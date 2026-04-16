# helpers/openrouter.ps1
# reads stdin, POSTs to OpenRouter (OpenAI-compatible), prints the reply.
# needs OPENROUTER_API_KEY in the environment.
#
# set per-session:   $env:OPENROUTER_API_KEY = 'sk-or-...'
# or persist:        setx OPENROUTER_API_KEY "sk-or-..."
#
# models use the provider/model format, e.g.:
#   anthropic/claude-3.5-sonnet
#   openai/gpt-4o-mini
#   meta-llama/llama-3.1-70b-instruct

param(
    [string]$Model  = 'openai/gpt-4o-mini',
    [string]$System = 'Reply concisely. Return just the rewritten text.'
)

$user = [Console]::In.ReadToEnd()

$body = @{
    model    = $Model
    messages = @(
        @{ role = 'system'; content = $System },
        @{ role = 'user';   content = $user }
    )
} | ConvertTo-Json -Depth 8 -Compress

$r = Invoke-RestMethod `
    -Uri 'https://openrouter.ai/api/v1/chat/completions' `
    -Method Post `
    -Headers @{ Authorization = "Bearer $env:OPENROUTER_API_KEY" } `
    -ContentType 'application/json; charset=utf-8' `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))

$r.choices[0].message.content
