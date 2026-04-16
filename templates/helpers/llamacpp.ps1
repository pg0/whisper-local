# helpers/llamacpp.ps1
# reads stdin, calls a local llama.cpp server (OpenAI-compatible), prints reply.
# default endpoint: http://localhost:8080/v1/chat/completions
# start with:  llama-server -m model.gguf --port 8080

param(
    [string]$Model = 'local-model',
    [string]$Url   = 'http://localhost:8080/v1/chat/completions'
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
