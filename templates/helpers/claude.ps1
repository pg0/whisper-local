# helpers/claude.ps1
# reads stdin (the selected text), pipes it to the `claude` CLI, prints the reply.
# invoke via a replace_map entry like:
#   ask claude:>>exec:powershell -NoProfile -File "%APPDATA%\whisper-local\helpers\claude.ps1"

param(
    [string]$Prompt = 'Reply concisely.',
    [string]$AllowedTools = 'Read,Edit,Bash'
)

$selection = [Console]::In.ReadToEnd()
$full = "$Prompt`n`n$selection"
$full | claude -p --allowedTools $AllowedTools -
