# helpers/openai.ps1
# reads stdin, POSTs to OpenAI chat completions, prints the assistant message.
# needs OPENAI_API_KEY in the environment.
#
# set the key once per terminal:
#   $env:OPENAI_API_KEY = 'sk-...'
# or persist for the current user (takes effect in new terminals):
#   setx OPENAI_API_KEY "sk-..."
# and check it:
#   echo $env:OPENAI_API_KEY

param(
    [string]$Model = 'gpt-4o-mini',
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
    -Uri 'https://api.openai.com/v1/chat/completions' `
    -Method Post `
    -Headers @{ Authorization = "Bearer $env:OPENAI_API_KEY" } `
    -ContentType 'application/json; charset=utf-8' `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))

$r.choices[0].message.content
