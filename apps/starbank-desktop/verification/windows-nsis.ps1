[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3.0

if (-not $IsWindows -or $env:GITHUB_ACTIONS -cne 'true' -or
    $env:RUNNER_ENVIRONMENT -cne 'github-hosted' -or $env:RUNNER_ARCH -cne 'X64') {
    throw 'This check requires a fresh GitHub-hosted Windows x64 runner.'
}

function Assert-PlainPath([System.IO.FileSystemInfo] $Item) {
    for ($entry = $Item; $null -ne $entry;) {
        if ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
            throw "Unexpected reparse point at $($entry.FullName)."
        }
        $entry = if ($entry -is [System.IO.DirectoryInfo]) { $entry.Parent } else { $entry.Directory }
    }
}

function Get-PlainFile([string] $Path) {
    $item = Get-Item -LiteralPath $Path -Force
    Assert-PlainPath $item
    if ($item -isnot [System.IO.FileInfo]) { throw "Expected a plain file at $Path." }
    return $item
}

function Get-PlainFileHash([string] $Path) {
    return (Get-FileHash -LiteralPath (Get-PlainFile $Path).FullName -Algorithm SHA256).Hash
}

function Get-ExpectedNsisExecutable([string] $Path) {
    $bytes = [System.IO.File]::ReadAllBytes((Get-PlainFile $Path).FullName)
    $sourceHash = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($bytes))
    $installerHash = $sourceHash
    # Tauri CLI 2.12.6 changes the first bundle marker before embedding the executable.
    # Latin-1 preserves byte offsets while locating the ASCII marker.
    $marker = '__TAURI_BUNDLE_TYPE_VAR_UNK'
    $offset = [System.Text.Encoding]::Latin1.GetString($bytes).IndexOf($marker, [StringComparison]::Ordinal)
    if ($offset -ge 0) {
        [System.Text.Encoding]::ASCII.GetBytes('__TAURI_BUNDLE_TYPE_VAR_NSS').CopyTo($bytes, $offset)
        $installerHash = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($bytes))
    }
    return @{ sourceHash = $sourceHash; installerHash = $installerHash; markerOffset = $offset }
}

function Get-ReportedFileHash([string] $Path) {
    try {
        if (Test-Path -LiteralPath $Path) { return Get-PlainFileHash $Path }
    } catch {
        return 'unavailable'
    }
    return $null
}

function Get-ReportedRegistryValue([string] $SubKey, [string] $Name) {
    try {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey)
        if ($null -eq $key) { return @{ kind = 'MissingKey'; value = $null } }
        try {
            if ($key.GetValueNames() -notcontains $Name) { return @{ kind = 'MissingValue'; value = $null } }
            $kind = $key.GetValueKind($Name).ToString()
            $value = if ($kind -in @('String', 'ExpandString')) {
                $key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            }
            return @{ kind = $kind; value = $value }
        } finally {
            $key.Dispose()
        }
    } catch {
        return @{ kind = 'Unreadable'; value = $null }
    }
}

function Assert-RegistryString([string] $SubKey, [string] $Name, [string] $Expected) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey)
    if ($null -eq $key) { throw "Missing HKCU\$SubKey." }
    try {
        if ($key.GetValue($Name) -cne $Expected -or
            $key.GetValueKind($Name) -ne [Microsoft.Win32.RegistryValueKind]::String) {
            throw "Unexpected HKCU\$SubKey registry value '$Name'."
        }
    } finally {
        $key.Dispose()
    }
}

function Assert-NoRegistryValue([string] $SubKey, [string] $Name) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($SubKey)
    if ($null -eq $key) { return }
    try {
        if ($key.GetValueNames() -contains $Name) { throw "Unexpected HKCU\$SubKey registry value '$Name'." }
    } finally {
        $key.Dispose()
    }
}

function Clear-OwnedRegistryString([string] $SubKey, [string] $Name, [string] $Expected) {
    $registry = [Microsoft.Win32.Registry]::CurrentUser
    $key = $registry.OpenSubKey($SubKey, $true)
    if ($null -eq $key) { return }
    try {
        if ($key.GetValue($Name) -ceq $Expected -and
            $key.GetValueKind($Name) -eq [Microsoft.Win32.RegistryValueKind]::String) {
            $key.DeleteValue($Name)
        }
        $empty = $key.ValueCount -eq 0 -and $key.SubKeyCount -eq 0
    } finally {
        $key.Dispose()
    }
    if ($empty) { $registry.DeleteSubKey($SubKey) }
}

function New-PreservationFixture([string] $Root) {
    if (Test-Path -LiteralPath $Root) { throw 'The application-data fixture must start absent.' }
    $parent = Get-Item -LiteralPath ([System.IO.Path]::GetDirectoryName($Root)) -Force
    Assert-PlainPath $parent
    $directory = New-Item -Path $parent.FullName -Name ([System.IO.Path]::GetFileName($Root)) -ItemType Directory
    Assert-PlainPath $directory
    $backupDirectory = New-Item -Path $directory.FullName -Name 'Backups' -ItemType Directory
    Assert-PlainPath $backupDirectory
    # Opaque byte-preservation sentinels; no catalog validity or restore is claimed.
    $sentinels = [ordered]@{
        'settings.json' = '{"version":1,"syntheticInstallerPreservation":true}'
        'Backups\Synthetic_20000101_000000.sc2bank.bak' = '<Bank version="1"><Section name="Installer preservation"/></Bank>'
        'Backups\Synthetic_20000101_000000.sc2bank.bak.json' = '{"syntheticInstallerPreservation":"backup metadata bytes"}'
        'Backups\.starbank-pending-nsis-fixture' = '<Bank version="1"><Section name="Retained recovery bytes"/></Bank>'
    }
    $files = @()
    foreach ($entry in $sentinels.GetEnumerator()) {
        $path = Join-Path $directory.FullName $entry.Key
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($entry.Value)
        $file = [System.IO.File]::Open($path, [System.IO.FileMode]::CreateNew,
            [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
        try { $file.Write($bytes, 0, $bytes.Length) } finally { $file.Dispose() }
        $files += @{ relative = $entry.Key; path = $path; hash = Get-PlainFileHash $path }
    }
    return @{ root = $directory.FullName; backupDirectory = $backupDirectory.FullName; files = $files }
}

function Assert-PreservedFiles([hashtable] $Fixture, [string] $Phase) {
    foreach ($file in $Fixture.files) {
        if ((Get-PlainFileHash $file.path) -cne $file.hash) {
            throw "$Phase changed the synthetic app-data fixture $($file.relative)."
        }
    }
}

function Clear-PreservationFixture([hashtable] $Fixture, [string] $ExpectedRoot) {
    if ($Fixture.root -ine $ExpectedRoot -or
        $Fixture.backupDirectory -ine (Join-Path $ExpectedRoot 'Backups')) {
        throw 'Refusing to clean an unexpected app-data namespace.'
    }
    $root = Get-Item -LiteralPath $ExpectedRoot -Force
    $backups = Get-Item -LiteralPath $Fixture.backupDirectory -Force
    if (-not $root.PSIsContainer -or -not $backups.PSIsContainer) {
        throw 'App-data fixture directories changed type.'
    }
    Assert-PlainPath $root
    Assert-PlainPath $backups
    Assert-PreservedFiles $Fixture 'Before owned cleanup'
    $expected = @($Fixture.files | ForEach-Object { $_.path }) + @($backups.FullName)
    # Inspect only the two known directories; never recurse through an unexpected entry.
    $actual = @(Get-ChildItem -LiteralPath $root.FullName -Force) +
        @(Get-ChildItem -LiteralPath $backups.FullName -Force)
    if ($actual.Count -ne $expected.Count) { throw 'Unexpected files in the app-data fixture; retaining evidence.' }
    foreach ($entry in $actual) {
        Assert-PlainPath $entry
        if ($expected -inotcontains $entry.FullName) {
            throw 'Unexpected app-data fixture entry; refusing cleanup.'
        }
    }
    foreach ($file in $Fixture.files) {
        if ($file.path -ine (Join-Path $ExpectedRoot $file.relative)) {
            throw 'App-data fixture path escaped its recorded namespace.'
        }
    }
    foreach ($file in $Fixture.files) { Remove-Item -LiteralPath $file.path }
    # Nonrecursive removal refuses any newly introduced or non-owned content.
    [System.IO.Directory]::Delete($backups.FullName, $false)
    [System.IO.Directory]::Delete($root.FullName, $false)
}

function Invoke-SilentNsis([string] $Executable, [string] $Arguments) {
    $file = Get-PlainFile $Executable
    if ($file.Extension -cne '.exe') { throw 'Expected an NSIS executable.' }
    $process = Start-Process -FilePath $file.FullName -ArgumentList $Arguments -WindowStyle Hidden -Wait -PassThru
    try {
        if ($process.ExitCode -ne 0) { throw "$($file.Name) exited with code $($process.ExitCode)." }
    } finally {
        $process.Dispose()
    }
}

$workspace = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
if ($workspace -ine [System.IO.Path]::GetFullPath($env:GITHUB_WORKSPACE)) {
    throw 'The script must run from the checked-out workspace.'
}
if ($env:RUNNER_TEMP -notmatch '^[A-Za-z]:\\') { throw 'RUNNER_TEMP must be an absolute local path.' }
$temp = Get-Item -LiteralPath $env:RUNNER_TEMP -Force
if (-not $temp.PSIsContainer -or $null -eq $temp.Parent) { throw 'RUNNER_TEMP must be a non-root directory.' }
Assert-PlainPath $temp

$product = 'StarBank Reloaded'
$bundleId = 'org.starbank-reloaded.desktop'
$mainBinary = 'starbank-desktop.exe'
$uninstaller = 'uninstall.exe'
$progId = 'StarBankReloaded.SC2Bank'
$extensionKey = 'Software\Classes\.sc2bank'
$openWithKey = "$extensionKey\OpenWithProgids"
$classKey = "Software\Classes\$progId"
$userChoiceKey = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.sc2bank\UserChoice'
$uninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$product"
$publisherKey = 'Software\starbank-reloaded'
$runKey = 'Software\Microsoft\Windows\CurrentVersion\Run'
$installLocationKey = "$publisherKey\$product"
$defaultHandler = "StarBankNsisTest.$([guid]::NewGuid().ToString('N').Substring(0, 16))"

$configDir = Join-Path $PSScriptRoot '../src-tauri'
$config = Get-Content -LiteralPath (Join-Path $configDir 'tauri.conf.json') -Raw | ConvertFrom-Json
$windows = Get-Content -LiteralPath (Join-Path $configDir 'tauri.windows.conf.json') -Raw | ConvertFrom-Json
if ($config.identifier -cne $bundleId -or $config.productName -cne $product -or
    $windows.bundle.targets.Count -ne 1 -or $windows.bundle.targets[0] -cne 'nsis' -or
    $windows.bundle.fileAssociations.Count -ne 0 -or
    $windows.bundle.windows.nsis.installMode -cne 'currentUser' -or
    $windows.bundle.windows.nsis.installerHooks -cne 'windows/installer-hooks.nsh') {
    throw 'The bundle product, identifier or per-user NSIS hook configuration changed.'
}

# Tauri 2.12.7 app_config_dir/app_data_dir append the identifier to the OS
# config/data folder. Both are RoamingAppData on Windows; overrides change that contract.
foreach ($bundleConfig in @($config, $windows)) {
    $identifierOverride = $bundleConfig.PSObject.Properties['identifier']
    if ($null -ne $identifierOverride -and $identifierOverride.Value -cne $bundleId) {
        throw 'Bundle identifier overrides require an updated preservation fixture.'
    }
    $appConfig = $bundleConfig.PSObject.Properties['app']
    foreach ($overrideKey in @('appDirectoriesOverride', 'app-directories-override')) {
        if ($null -ne $appConfig -and
            $null -ne $appConfig.Value.PSObject.Properties[$overrideKey]) {
            throw 'App directory overrides require an updated preservation fixture.'
        }
    }
}
$roamingPath = [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)
if ($roamingPath -notmatch '^[A-Za-z]:\\' -or
    [System.IO.Path]::GetFullPath($roamingPath) -ine [System.IO.Path]::GetFullPath($env:APPDATA)) {
    throw 'APPDATA must match the OS RoamingAppData namespace used by Tauri.'
}
$roaming = Get-Item -LiteralPath $roamingPath -Force
if (-not $roaming.PSIsContainer) { throw 'RoamingAppData must be an existing directory.' }
Assert-PlainPath $roaming
$appDataNamespace = Join-Path $roaming.FullName $bundleId

$buildDir = Join-Path $workspace 'target/debug'
$expectedExecutable = Get-ExpectedNsisExecutable (Join-Path $buildDir $mainBinary)
$bundleDir = Join-Path $buildDir 'bundle/nsis'
$installers = @(Get-ChildItem -LiteralPath $bundleDir -Filter '*-setup.exe' -Force -File)
$expectedInstaller = '{0}_{1}_x64-setup.exe' -f $product, $config.version
if ($installers.Count -ne 1 -or $installers[0].Name -cne $expectedInstaller) {
    throw "Expected exactly one fresh bundle named $expectedInstaller."
}
$installer = $installers[0]
Assert-PlainPath $installer
if ((Get-AuthenticodeSignature -LiteralPath $installer.FullName).Status -ne 'NotSigned') {
    throw 'The CI installer must be unsigned.'
}

$machineKeys = @(
    "HKLM:\$extensionKey", "HKLM:\$classKey", "HKLM:\$uninstallKey",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$product"
)
$priorState = @(
    "HKCU:\$extensionKey", "HKCU:\$classKey", "HKCU:\$userChoiceKey", "HKCU:\$uninstallKey", "HKCU:\$publisherKey",
    (Join-Path $env:APPDATA $bundleId), (Join-Path $env:LOCALAPPDATA $bundleId), (Join-Path $env:LOCALAPPDATA $product)
) + $machineKeys
foreach ($path in $priorState) {
    if (Test-Path -LiteralPath $path) { throw "The runner already has state at $path." }
}

Assert-NoRegistryValue $runKey $product

$workName = "starbank-nsis-$([guid]::NewGuid().ToString('N'))"
$work = New-Item -Path $temp.FullName -Name $workName -ItemType Directory
$installDir = Join-Path $work.FullName 'Install With Spaces'
$installedApp = Join-Path $installDir $mainBinary
$installedUninstaller = Join-Path $installDir $uninstaller
$fixture = Join-Path $installDir 'Synthetic Fixture.SC2Bank'
$extensionCreated = $false
$fixtureHash = $null
$installedHash = $null
$preservation = $null
$phase = 'prepare'
$qualified = $false
$openCommandKey = "$classKey\shell\open\command"
$openCommand = '"{0}" "%1"' -f $installedApp

try {
    Assert-PlainPath $work
    $null = New-Item -Path $installDir -ItemType Directory
    [System.IO.File]::WriteAllText($fixture, '<Bank version="1"></Bank>')
    $fixtureHash = Get-PlainFileHash $fixture
    $preservation = New-PreservationFixture $appDataNamespace

    $extension = New-Item -Path "HKCU:\$extensionKey"
    $extensionCreated = $true
    try {
        $extension.SetValue('', $defaultHandler, [Microsoft.Win32.RegistryValueKind]::String)
        $openWith = $extension.CreateSubKey('OpenWithProgids')
        try {
            $openWith.SetValue($defaultHandler, '', [Microsoft.Win32.RegistryValueKind]::String)
        } finally {
            $openWith.Dispose()
        }
    } finally {
        $extension.Dispose()
    }

    # Tauri /NS suppresses shortcuts. NSIS requires /D to be last and unquoted.
    $phase = 'install'
    Invoke-SilentNsis $installer.FullName "/S /NS /D=$installDir"
    $installedHash = Get-PlainFileHash $installedApp
    if ($installedHash -cne $expectedExecutable.installerHash) {
        throw 'The installed executable differs from the build output with the expected NSIS bundle marker.'
    }
    Assert-PlainPath (Get-Item -LiteralPath $installedUninstaller -Force)
    if ((Get-PlainFileHash $fixture) -cne $fixtureHash) {
        throw 'Installation changed the synthetic bank.'
    }
    Assert-PreservedFiles $preservation 'Installation'
    Assert-RegistryString $classKey '' 'StarCraft II Bank'
    Assert-RegistryString "$classKey\DefaultIcon" '' ('"{0}",0' -f $installedApp)
    Assert-RegistryString $openCommandKey '' $openCommand
    Assert-RegistryString $openWithKey $progId ''
    Assert-RegistryString $openWithKey $defaultHandler ''
    Assert-RegistryString $extensionKey '' $defaultHandler
    Assert-RegistryString $uninstallKey 'DisplayName' $product
    Assert-RegistryString $uninstallKey 'DisplayVersion' $config.version
    Assert-RegistryString $uninstallKey 'Publisher' 'starbank-reloaded'
    Assert-RegistryString $uninstallKey 'UninstallString' ('"{0}"' -f $installedUninstaller)
    Assert-RegistryString $installLocationKey '' $installDir
    Assert-NoRegistryValue $runKey $product
    foreach ($path in @("HKCU:\$userChoiceKey") + $machineKeys) {
        if (Test-Path -LiteralPath $path) { throw "Unexpected registry key after install at $path." }
    }

    # Run a copy to wait for the real exit status and let NSIS delete the installed uninstaller.
    $uninstallCopy = Join-Path $work.FullName $uninstaller
    Copy-Item -LiteralPath $installedUninstaller -Destination $uninstallCopy
    $phase = 'uninstall'
    Invoke-SilentNsis $uninstallCopy "/S _?=$installDir"

    foreach ($path in @("HKCU:\$classKey", "HKCU:\$uninstallKey", "HKCU:\$userChoiceKey") + $machineKeys) {
        if (Test-Path -LiteralPath $path) { throw "Unexpected registry key after uninstall at $path." }
    }
    $entries = @(Get-ChildItem -LiteralPath $installDir -Force)
    if ($entries.Count -ne 1 -or $entries[0].FullName -ine $fixture -or $entries[0].PSIsContainer) {
        throw 'Uninstall must remove the installed files and leave the synthetic bank.'
    }
    if ((Get-PlainFileHash $fixture) -cne $fixtureHash) {
        throw 'Uninstall changed the synthetic bank.'
    }
    Assert-PreservedFiles $preservation 'Uninstall'
    Assert-NoRegistryValue $openWithKey $progId
    Assert-RegistryString $openWithKey $defaultHandler ''
    Assert-RegistryString $extensionKey '' $defaultHandler
    $qualified = $true
} catch {
    Write-Output "Retaining failed NSIS test files at $($work.FullName) and app-data fixture at $appDataNamespace."
    $report = [ordered]@{
        phase = $phase
        appDataPreservation = @(
            if ($null -ne $preservation) {
                foreach ($file in $preservation.files) {
                    @{ file = $file.relative; expected = $file.hash; actual = Get-ReportedFileHash $file.path }
                }
            }
        )
        bankSha256 = @{ expected = $fixtureHash; actual = Get-ReportedFileHash $fixture }
        builtExecutableSha256 = $expectedExecutable.sourceHash
        nsisBundleMarkerOffset = $expectedExecutable.markerOffset
        installedExecutableSha256 = @{
            expected = if ($phase -eq 'install') { $expectedExecutable.installerHash } else { $null }
            actual = Get-ReportedFileHash $installedApp
            afterInstall = $installedHash
        }
        extensionDefault = @{
            expected = $defaultHandler
            actual = Get-ReportedRegistryValue $extensionKey ''
        }
        sentinelOpenWith = @{
            expected = ''
            actual = Get-ReportedRegistryValue $openWithKey $defaultHandler
        }
        applicationOpenWith = @{
            progId = $progId
            expected = if ($phase -eq 'install') { '' } else { $null }
            actual = Get-ReportedRegistryValue $openWithKey $progId
        }
        openCommand = @{
            expected = if ($phase -eq 'install') { $openCommand } else { $null }
            actual = Get-ReportedRegistryValue $openCommandKey ''
        }
    }
    Write-Output ($report | ConvertTo-Json -Depth 4 -Compress)
    throw
} finally {
    try {
        if ($extensionCreated) {
            Clear-OwnedRegistryString $openWithKey $defaultHandler ''
            Clear-OwnedRegistryString $extensionKey '' $defaultHandler
        }
        # Tauri remembers the install location after a normal silent uninstall.
        Clear-OwnedRegistryString $installLocationKey '' $installDir
        Clear-OwnedRegistryString $publisherKey '' $installDir
    } catch {
        Write-Warning "Could not finish clearing the NSIS registry fixtures. Retaining files at $($work.FullName)."
        if ($qualified) { throw }
    }
    if ($qualified) {
        try {
            Clear-PreservationFixture $preservation $appDataNamespace
        } catch {
            Write-Warning "App-data fixture cleanup refused; retaining evidence at $appDataNamespace and $($work.FullName)."
            throw
        }
        $root = Get-Item -LiteralPath $work.FullName -Force
        if ($root.Name -cne $workName -or $root.Parent.FullName -ine $temp.FullName) {
            throw 'Refusing to clean a directory outside the owned runner temp path.'
        }
        Assert-PlainPath $root
        foreach ($entry in Get-ChildItem -LiteralPath $root.FullName -Force -Recurse) {
            if ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw 'Refusing to clean a test directory containing a reparse point.'
            }
        }
        Remove-Item -LiteralPath $root.FullName -Force -Recurse
    }
}

Write-Output 'Silent NSIS install and uninstall verified OpenWith registration, the default-handler sentinel, the bank SHA-256, settings/backup/recovery fixture hashes and the executable SHA-256 after Tauri bundle metadata.'
