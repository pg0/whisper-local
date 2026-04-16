# helpers/ollama.ps1
# reads stdin, calls the local Ollama chat endpoint, prints the reply.
# default endpoint: http://localhost:11434/api/chat

param(
    [string]$Model = 'llama3',
    [string]$Url   = 'http://localhost:11434/api/chat'
)

$user = [Console]::In.ReadToEnd()

$body = @{
    model    = $Model
    stream   = $false
    messages = @(@{ role = 'user'; content = $user })
} | ConvertTo-Json -Depth 8 -Compress

$r = Invoke-RestMethod `
    -Uri $Url `
    -Method Post `
    -ContentType 'application/json; charset=utf-8' `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))

$r.message.content
