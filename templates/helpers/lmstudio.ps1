# helpers/lmstudio.ps1
# reads stdin, calls LM Studio's OpenAI-compatible endpoint, prints the reply.
# default endpoint: http://localhost:1234/v1/chat/completions

param(
    [string]$Model = 'local-model',
    [string]$Url   = 'http://localhost:1234/v1/chat/completions'
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
