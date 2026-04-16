# helpers/vllm.ps1
# reads stdin, calls a local vLLM OpenAI-compatible endpoint, prints the reply.
# default endpoint: http://localhost:8000/v1/chat/completions
# start vLLM with:  python -m vllm.entrypoints.openai.api_server --model <repo>
#
# set per-session:   $env:VLLM_MODEL = 'meta-llama/Meta-Llama-3-8B-Instruct'
# or persist:        setx VLLM_MODEL "meta-llama/Meta-Llama-3-8B-Instruct"

param(
    [string]$Model = $(if ($env:VLLM_MODEL) { $env:VLLM_MODEL } else { 'local-model' }),
    [string]$Url   = 'http://localhost:8000/v1/chat/completions'
)

$user = [Console]::In.ReadToEnd()

$body = @{
    model    = $Model
    messages = @(@{ role = 'user'; content = $user })
} | ConvertTo-Json -Depth 8 -Compress

$r = Invoke-RestMethod `
    -Uri $Url `
    -Method Post `
    -ContentType 'application/json; charset=utf-8' `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))

$r.choices[0].message.content
