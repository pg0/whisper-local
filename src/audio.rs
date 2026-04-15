use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::sync::Arc;

const TARGET_RATE: u32 = 16_000;
const MAX_SECONDS: usize = 120;

pub struct AudioCapture {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    input_rate: u32,
    channels: u16,
    pub rms_rx: Receiver<f32>,
}

impl AudioCapture {
    pub fn list_input_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|iter| iter.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    pub fn start(preferred: &str) -> Result<Self> {
        let host = cpal::default_host();
        let device = if preferred.is_empty() {
            host.default_input_device()
                .context("no default input device")?
        } else {
            host.input_devices()?
                .find(|d| d.name().map(|n| n == preferred).unwrap_or(false))
                .or_else(|| host.default_input_device())
                .context("no input device")?
        };
        let default = device
            .default_input_config()
            .context("default_input_config")?;
        let input_rate = default.sample_rate().0;
        let channels = default.channels();
        let cfg: StreamConfig = default.clone().into();

        let buf = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
            input_rate as usize * MAX_SECONDS,
        )));
        let buf_cb = buf.clone();
        let (rms_tx, rms_rx) = bounded::<f32>(64);

        let sample_fmt = default.sample_format();
        let stream = match sample_fmt {
            SampleFormat::F32 => device.build_input_stream(
                &cfg,
                move |data: &[f32], _| {
                    capture_f32(data, channels, &buf_cb, &rms_tx, input_rate)
                },
                move |err| log::error!("cpal input err: {err}"),
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &cfg,
                move |data: &[i16], _| {
                    let fdata: Vec<f32> = data
                        .iter()
                        .map(|&s| s as f32 / i16::MAX as f32)
                        .collect();
                    capture_f32(&fdata, channels, &buf_cb, &rms_tx, input_rate);
                },
                move |err| log::error!("cpal input err: {err}"),
                None,
            )?,
            fmt => return Err(anyhow!("unsupported sample format {:?}", fmt)),
        };
        stream.play()?;

        Ok(Self {
            stream,
            buf,
            input_rate,
            channels,
            rms_rx,
        })
    }

    /// Stop the stream and produce a 16 kHz mono WAV byte vector.
    pub fn stop(self) -> Result<Vec<u8>> {
        drop(self.stream); // stops capture
        let samples = Arc::try_unwrap(self.buf)
            .map_err(|_| anyhow!("buf still shared"))
            .map(|m| m.into_inner())?;
        let resampled = resample_to_16k(samples, self.input_rate)?;
        encode_wav(&resampled)
    }
}

fn capture_f32(
    data: &[f32],
    channels: u16,
    buf: &Arc<Mutex<Vec<f32>>>,
    rms_tx: &Sender<f32>,
    input_rate: u32,
) {
    let mono: Vec<f32> = if channels == 1 {
        data.to_vec()
    } else {
        data.chunks(channels as usize)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if !mono.is_empty() {
        let sumsq: f32 = mono.iter().map(|s| s * s).sum();
        let rms = (sumsq / mono.len() as f32).sqrt();
        let _ = rms_tx.try_send(rms);
    }

    let mut b = buf.lock();
    let cap = input_rate as usize * MAX_SECONDS;
    if b.len() + mono.len() > cap {
        let drop = (b.len() + mono.len()) - cap;
        b.drain(0..drop);
    }
    b.extend_from_slice(&mono);
}

fn resample_to_16k(input: Vec<f32>, input_rate: u32) -> Result<Vec<f32>> {
    if input_rate == TARGET_RATE {
        return Ok(input);
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let params = SincInterpolationParameters {
        sinc_len: 128,
        f_cutoff: 0.95,
        oversampling_factor: 128,
        interpolation: SincInterpolationType::Linear,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk_size = input.len();
    let mut r = SincFixedIn::<f32>::new(
        TARGET_RATE as f64 / input_rate as f64,
        1.0,
        params,
        chunk_size,
        1, // mono
    )?;
    // process expects &[V] where V: AsRef<[T]>; pass single-channel slice
    let out = r.process(&[input], None)?;
    Ok(out.into_iter().next().unwrap_or_default())
}

pub fn encode_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: TARGET_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::new(&mut buf, spec)?;
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            w.write_sample(v)?;
        }
        w.finalize()?;
    }
    Ok(buf.into_inner())
}
