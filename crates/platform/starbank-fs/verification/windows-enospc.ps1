# Qualify the disabled Windows candidate on a VHD owned by a disposable hosted job.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows -or $env:GITHUB_ACTIONS -cne 'true' -or $env:RUNNER_ENVIRONMENT -cne 'github-hosted') {
    throw 'This fixture requires a disposable GitHub-hosted Windows runner.'
}
$runnerIdentity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($runnerIdentity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'This fixture requires the existing elevated runner token.'
}
$runnerUser = $runnerIdentity.User.Value
if ($runnerUser -cnotmatch '^S-1-(?:[0-9]+-)*[0-9]+$') {
    throw 'The existing runner token must have a valid user SID.'
}

$capacity = 128MB
$minimumNtfsCapacity = 112MB
$fixedVhdFooter = 512
$fileBackedVirtualBus = 15
$runnerTemp = [IO.Path]::GetFullPath($env:RUNNER_TEMP)
if ($runnerTemp -cnotmatch '^[A-Za-z]:\\[A-Za-z0-9 ._\\-]+$') {
    throw 'RUNNER_TEMP must be an ordinary local path that DiskPart can represent.'
}
for ($directory = [IO.DirectoryInfo]::new($runnerTemp); $null -ne $directory; $directory = $directory.Parent) {
    if (-not $directory.Exists -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'RUNNER_TEMP must already exist without reparse points.'
    }
}

$hostReserve = 512MB
$hostDrive = [IO.DriveInfo]::new([IO.Path]::GetPathRoot($runnerTemp))
if (-not $hostDrive.IsReady -or $hostDrive.DriveType -ne [IO.DriveType]::Fixed -or
    $hostDrive.AvailableFreeSpace -lt ($capacity + $fixedVhdFooter + $hostReserve)) {
    throw 'The local backing drive lacks the reserved free space for this fixed VHD.'
}
Get-Command Get-Acl, Mount-DiskImage, Get-DiskImage, Get-Disk, Initialize-Disk, New-Partition, Get-Partition, Format-Volume, Get-Volume, Dismount-DiskImage | Out-Null

$nonce = [Guid]::NewGuid().ToString('N')
$label = 'SBENOSPC-' + $nonce.Substring(0, 8)
$scratch = Join-Path $runnerTemp ('starbank-enospc-' + $nonce)
New-Item -ItemType Directory -Path $scratch | Out-Null
$image = Join-Path $scratch 'owned.vhd'
$diskpartScript = Join-Path $scratch 'create-vhd.txt'
$mountAttempted = $false

function Get-OwnedImage {
    $images = @(Get-DiskImage -ImagePath $image)
    if ($images.Count -ne 1 -or $images[0].ImagePath -ine $image) {
        throw 'The storage provider did not resolve the exact owned VHD.'
    }
    return $images[0]
}

function Get-OwnedDisk {
    $attached = Get-OwnedImage
    if ($attached.Attached -ne $true -or $attached.DevicePath -notmatch '^\\\\\.\\PhysicalDrive[0-9]+$') {
        throw 'The owned image did not attach as one disk device.'
    }
    $disks = @($attached | Get-Disk)
    if ($disks.Count -ne 1) {
        throw 'The mounted image must resolve to exactly one disk.'
    }
    $disk = $disks[0]
    if ($disk.Size -ne $capacity -or
        $disk.CimInstanceProperties['BusType'].Value -ne $fileBackedVirtualBus -or
        $attached.Number -ne $disk.Number -or
        $attached.DevicePath -ine ('\\.\PhysicalDrive{0}' -f $disk.Number) -or
        $disk.IsBoot -ne $false -or $disk.IsSystem -ne $false -or
        $disk.IsOffline -ne $false -or $disk.IsReadOnly -ne $false -or
        [string]::IsNullOrWhiteSpace($disk.ObjectId)) {
        throw 'The image did not resolve to the expected bounded, online, file-backed virtual disk.'
    }
    return $disk
}

try {
    if (Test-Path -LiteralPath $image) {
        throw 'The fixture backing file must start absent.'
    }
    # A fixed VHD reserves the backing space before the guest NTFS volume can be filled.
    $create = 'create vdisk file="{0}" maximum=128 type=fixed sd=D:P(A;;FA;;;{1})' -f $image, $runnerUser
    [IO.File]::WriteAllLines($diskpartScript, [string[]]@($create, 'exit'), [Text.Encoding]::ASCII)
    # Do not emit the DiskPart command or its descriptor if the provider echoes it.
    & (Join-Path $env:SystemRoot 'System32\diskpart.exe') /s $diskpartScript 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "DiskPart failed to create the owned fixed VHD ($LASTEXITCODE)."
    }
    $backing = Get-Item -LiteralPath $image
    $special = [IO.FileAttributes]::ReparsePoint -bor [IO.FileAttributes]::SparseFile -bor [IO.FileAttributes]::Compressed
    if ($backing.PSIsContainer -or ($backing.Attributes -band $special) -or
        $backing.Length -ne ($capacity + $fixedVhdFooter)) {
        throw 'The backing file must be the expected fixed-size VHD.'
    }
    $backingAcl = Get-Acl -LiteralPath $image
    $rules = @($backingAcl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if (-not $backingAcl.AreAccessRulesProtected -or $rules.Count -ne 1 -or
        $rules[0].IdentityReference.Value -cne $runnerUser -or $rules[0].IsInherited -or
        $rules[0].AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
        $rules[0].FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl -or
        $rules[0].InheritanceFlags -ne [Security.AccessControl.InheritanceFlags]::None -or
        $rules[0].PropagationFlags -ne [Security.AccessControl.PropagationFlags]::None) {
        throw 'The owned VHD did not retain its exact protected one-user DACL.'
    }
    $sections = [Security.AccessControl.AccessControlSections]::Owner -bor
        [Security.AccessControl.AccessControlSections]::Group -bor [Security.AccessControl.AccessControlSections]::Access
    $backingSddl = $backingAcl.GetSecurityDescriptorSddlForm($sections)
    Write-Host 'Dedicated VHD backing ACL readback: protected exact runner-user full control'

    $mountAttempted = $true
    Mount-DiskImage -ImagePath $image -StorageType VHD -Access ReadWrite -NoDriveLetter | Out-Null
    $raw = Get-OwnedDisk
    if ($raw.PartitionStyle -cne 'RAW' -or $raw.NumberOfPartitions -ne 0) {
        throw 'Only a fresh unpartitioned owned VHD may be initialized.'
    }
    Initialize-Disk -InputObject $raw -PartitionStyle MBR -Confirm:$false
    $initialized = Get-OwnedDisk
    if ($initialized.ObjectId -cne $raw.ObjectId -or $initialized.PartitionStyle -cne 'MBR' -or
        $initialized.NumberOfPartitions -ne 0) {
        throw 'The initialized disk did not remain the same owned VHD.'
    }
    $created = @(New-Partition -InputObject $initialized -UseMaximumSize -AssignDriveLetter)
    $ownedDisk = Get-OwnedDisk
    if ($ownedDisk.ObjectId -cne $initialized.ObjectId -or $ownedDisk.NumberOfPartitions -ne 1) {
        throw 'The owned VHD must have exactly one new partition.'
    }
    $partitions = @(Get-Partition -Disk $ownedDisk)
    if ($created.Count -ne 1 -or $partitions.Count -ne 1 -or
        [string]::IsNullOrWhiteSpace($partitions[0].ObjectId) -or
        $partitions[0].ObjectId -cne $created[0].ObjectId -or $partitions[0].DiskNumber -ne $ownedDisk.Number -or
        $partitions[0].IsBoot -ne $false -or $partitions[0].IsSystem -ne $false -or
        $partitions[0].Size -lt $minimumNtfsCapacity -or $partitions[0].Size -gt $capacity) {
        throw 'The partition to format must be the single bounded partition returned for the owned VHD.'
    }
    $partition = $partitions[0]
    Format-Volume -Partition $partition -FileSystem NTFS -NewFileSystemLabel $label -Confirm:$false | Out-Null
    $volumes = @(Get-Volume -Partition $partition)
    if ($volumes.Count -ne 1) {
        throw 'The owned partition must resolve to exactly one formatted volume.'
    }
    $volume = $volumes[0]
    if ($volume.FileSystem -cne 'NTFS' -or $volume.FileSystemLabel -cne $label -or
        $volume.Size -lt $minimumNtfsCapacity -or $volume.Size -gt $capacity -or
        $volume.DriveLetter -cnotmatch '^[A-Za-z]$' -or [string]::IsNullOrWhiteSpace($volume.ObjectId) -or
        $volume.Path -notmatch '^\\\\\?\\Volume\{[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}\}\\$') {
        throw 'The fixture volume must have the expected size, label and NTFS volume identity.'
    }
    $mounted = @(Get-Volume -DriveLetter $volume.DriveLetter)
    $drive = '{0}:' -f $volume.DriveLetter
    if ($drive -ieq $env:SystemDrive -or $mounted.Count -ne 1 -or
        $mounted[0].ObjectId -cne $volume.ObjectId -or $mounted[0].Path -ine $volume.Path) {
        throw 'The assigned drive letter does not identify the owned NTFS volume.'
    }

    $markerPath = [IO.Path]::Combine($volume.Path, '.starbank-ntfs-enospc')
    $marker = [IO.File]::Open($markerPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
    try {
        $markerBytes = [Text.Encoding]::ASCII.GetBytes($nonce)
        $marker.Write($markerBytes, 0, $markerBytes.Length)
    } finally {
        $marker.Dispose()
    }
    $env:STARBANK_ENOSPC_NTFS_ROOT = $drive + '\'
    $env:STARBANK_ENOSPC_NTFS_VOLUME = $volume.Path
    $env:STARBANK_ENOSPC_NTFS_MARKER = $nonce
    $env:STARBANK_ENOSPC_NTFS_BACKING_SDDL = $backingSddl
    Write-Host "Dedicated NTFS VHD: virtual bytes=$capacity; volume bytes=$($volume.Size)"
    cargo nextest run --locked -p starbank-fs --lib --run-ignored only --no-tests fail --success-output immediate -E 'test(=windows::create::stage::enospc::bounded_ntfs_disk_full_rejects_before_publication)'
    if ($LASTEXITCODE -ne 0) {
        throw "The native NTFS disk-full test failed ($LASTEXITCODE)."
    }
} finally {
    Remove-Item Env:STARBANK_ENOSPC_NTFS_ROOT, Env:STARBANK_ENOSPC_NTFS_VOLUME,
        Env:STARBANK_ENOSPC_NTFS_MARKER, Env:STARBANK_ENOSPC_NTFS_BACKING_SDDL -ErrorAction SilentlyContinue
    if ($mountAttempted) {
        if ((Get-OwnedImage).Attached -eq $true) {
            Dismount-DiskImage -ImagePath $image | Out-Null
        }
        if ((Get-OwnedImage).Attached -ne $false) {
            throw 'Could not prove the owned VHD detached. Backing file retained on the disposable runner.'
        }
    }
    if (Test-Path -LiteralPath $image) {
        Remove-Item -LiteralPath $image
    }
    if (Test-Path -LiteralPath $diskpartScript) {
        Remove-Item -LiteralPath $diskpartScript
    }
    [IO.Directory]::Delete($scratch)
}
