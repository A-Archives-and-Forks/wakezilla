# Wakezilla Windows uninstaller bootstrap
# Bootstrap-Version: 1
#
# This version is intentionally non-operational. The complete Windows
# uninstall lifecycle will replace it in the dedicated lifecycle task. Until
# then, invoking this file must fail before touching files, registry settings,
# services, scheduled tasks, or user data.

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

throw 'Wakezilla uninstall is not implemented yet. No files, settings, or user data were changed.'
