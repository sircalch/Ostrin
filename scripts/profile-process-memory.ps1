[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
  [string]$BinaryPath,
  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
  [string]$CompilerPath,

  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
  [string]$SourcePath,

  [Parameter(Mandatory = $true)]
  [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
  [string]$FixturePath,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$ExpectedCompilerSha256,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$ExpectedOutputSha256,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{40}$')]
  [string]$SourceRevision,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{40}$')]
  [string]$TestedBinaryRevision,

  [ValidateRange(3, 100)]
  [int]$Runs = 7,

  [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
  throw 'This profiler currently supports Windows only.'
}
if ([IntPtr]::Size -ne 8) {
  throw 'Run this profiler from a 64-bit PowerShell process.'
}
if ($Runs % 2 -eq 0) {
  throw 'Runs must be odd so the reported median is unambiguous.'
}

$resolvedBinary = (Resolve-Path -LiteralPath $BinaryPath).Path
$expectedHash = $ExpectedOutputSha256.ToLowerInvariant()
$binaryHash = (Get-FileHash -LiteralPath $resolvedBinary -Algorithm SHA256).Hash.ToLowerInvariant()
$resolvedCompiler = (Resolve-Path -LiteralPath $CompilerPath).Path
$resolvedSource = (Resolve-Path -LiteralPath $SourcePath).Path
$resolvedFixture = (Resolve-Path -LiteralPath $FixturePath).Path
$compilerHash = (Get-FileHash -LiteralPath $resolvedCompiler -Algorithm SHA256).Hash.ToLowerInvariant()
$sourceHash = (Get-FileHash -LiteralPath $resolvedSource -Algorithm SHA256).Hash.ToLowerInvariant()
$fixtureHash = (Get-FileHash -LiteralPath $resolvedFixture -Algorithm SHA256).Hash.ToLowerInvariant()
if ($compilerHash -ne $ExpectedCompilerSha256.ToLowerInvariant()) {
  throw "Compiler SHA-256 $compilerHash differs from expected $ExpectedCompilerSha256."
}
$compilerVersion = (& $resolvedCompiler --version 2>&1 | Out-String).Trim()
if ($LASTEXITCODE -ne 0) { throw "Could not read compiler version from $resolvedCompiler." }
$runtimeInputPath = Join-Path ([IO.Path]::GetDirectoryName($resolvedBinary)) 'data.json'
if (-not (Test-Path -LiteralPath $runtimeInputPath -PathType Leaf)) {
  throw "The native workflow input snapshot is missing beside the binary: $runtimeInputPath"
}
$runtimeInputSha256 = (Get-FileHash -LiteralPath $runtimeInputPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($runtimeInputSha256 -ne $fixtureHash) {
  throw "Runtime input SHA-256 $runtimeInputSha256 differs from fixture $fixtureHash."
}

if (-not ('NativeMemoryProfile.Api' -as [type])) {
  $csharpSource = @(
    'using System;',
    'using System.Runtime.InteropServices;',
    'namespace NativeMemoryProfile {',
    '  [StructLayout(LayoutKind.Sequential)]',
    '  public struct CountersEx {',
    '    public uint cb;',
    '    public uint PageFaultCount;',
    '    public UIntPtr PeakWorkingSetSize;',
    '    public UIntPtr WorkingSetSize;',
    '    public UIntPtr QuotaPeakPagedPoolUsage;',
    '    public UIntPtr QuotaPagedPoolUsage;',
    '    public UIntPtr QuotaPeakNonPagedPoolUsage;',
    '    public UIntPtr QuotaNonPagedPoolUsage;',
    '    public UIntPtr PagefileUsage;',
    '    public UIntPtr PeakPagefileUsage;',
    '    public UIntPtr PrivateUsage;',
    '  }',
    '  public static class Api {',
    '    [DllImport("psapi.dll", SetLastError = true)]',
    '    [return: MarshalAs(UnmanagedType.Bool)]',
    '    public static extern bool GetProcessMemoryInfo(IntPtr process, ref CountersEx counters, uint size);',
    '  }',
    '}'
  )
  Add-Type -Language CSharp -TypeDefinition ($csharpSource -join [Environment]::NewLine)
}

$processCountersSize = [uint32][Runtime.InteropServices.Marshal]::SizeOf([type][NativeMemoryProfile.CountersEx])
$sampleRuns = [System.Collections.Generic.List[object]]::new()
$sha256 = [Security.Cryptography.SHA256]::Create()

for ($run = 0; $run -lt $Runs; $run++) {
  $startInfo = [Diagnostics.ProcessStartInfo]::new()
  $startInfo.FileName = $resolvedBinary
  $startInfo.WorkingDirectory = [IO.Path]::GetDirectoryName($resolvedBinary)
  $startInfo.UseShellExecute = $false
  $startInfo.CreateNoWindow = $true
  $startInfo.RedirectStandardOutput = $true
  $startInfo.RedirectStandardError = $true

  $process = [Diagnostics.Process]::Start($startInfo)
  $stdoutTask = $process.StandardOutput.ReadToEndAsync()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  [uint64]$peakWorkingSetBytes = 0
  [uint64]$peakPrivateBytes = 0
  $sampleCount = 0

  while (-not $process.HasExited) {
    $counters = [NativeMemoryProfile.CountersEx]::new()
    $counters.cb = $processCountersSize
    if ([NativeMemoryProfile.Api]::GetProcessMemoryInfo(
      $process.Handle,
      [ref]$counters,
      $processCountersSize
    )) {
      [uint64]$workingSetBytes = $counters.WorkingSetSize.ToUInt64()
      [uint64]$privateBytes = $counters.PrivateUsage.ToUInt64()
      if ($workingSetBytes -gt $peakWorkingSetBytes) { $peakWorkingSetBytes = $workingSetBytes }
      if ($privateBytes -gt $peakPrivateBytes) { $peakPrivateBytes = $privateBytes }
      $sampleCount++
    }
    [Threading.Thread]::SpinWait(100)
  }

  $process.WaitForExit()
  $exitCode = $process.ExitCode
  $stdout = $stdoutTask.GetAwaiter().GetResult()
  $stderr = $stderrTask.GetAwaiter().GetResult()
  $process.Dispose()

  if ($exitCode -ne 0) {
    throw "Native run $run failed with exit code $exitCode."
  }
  if (-not [string]::IsNullOrEmpty($stderr)) {
    throw "Native run $run wrote to stderr: $stderr"
  }
  if ($sampleCount -lt 2 -or $peakWorkingSetBytes -eq 0 -or $peakPrivateBytes -eq 0) {
    throw "Native run $run had insufficient live-process memory samples ($sampleCount)."
  }

  $actualHash = ([BitConverter]::ToString($sha256.ComputeHash([Text.Encoding]::UTF8.GetBytes($stdout)))).Replace('-', '').ToLowerInvariant()
  if ($actualHash -ne $expectedHash) {
    throw "Native run $run output SHA-256 $actualHash differs from expected $expectedHash."
  }

  $sampleRuns.Add([pscustomobject]@{
    run = $run + 1
    sampledPeakWorkingSetBytes = $peakWorkingSetBytes
    sampledPeakPrivateBytes = $peakPrivateBytes
    liveMemorySamples = $sampleCount
  })
}

$workingSetValues = @($sampleRuns | ForEach-Object { [uint64]$_.sampledPeakWorkingSetBytes } | Sort-Object)
$privateValues = @($sampleRuns | ForEach-Object { [uint64]$_.sampledPeakPrivateBytes } | Sort-Object)
$medianIndex = [int][Math]::Floor($Runs / 2)

$profile = [ordered]@{
  schema = 'ostrin.process-memory-profile/v1'
  generatedAtUtc = [DateTime]::UtcNow.ToString('o')
  sourceRevision = $SourceRevision.ToLowerInvariant()
  testedBinaryRevision = $TestedBinaryRevision.ToLowerInvariant()
  binarySha256 = $binaryHash
  compilerVersion = $compilerVersion
  compilerSha256 = $compilerHash
  workloadSourceSha256 = $sourceHash
  fixtureSha256 = $fixtureHash
  runtimeInputSha256 = $runtimeInputSha256
  backend = 'native'
  workloadOutputSha256 = $expectedHash
  host = [ordered]@{
    os = "Windows $([Environment]::OSVersion.Version)"
    architecture = if ([Environment]::Is64BitOperatingSystem) { 'x64' } else { 'x86' }
    processorCount = [Environment]::ProcessorCount
  }
  method = 'Launch one native child process per run; poll GetProcessMemoryInfo with a busy-spin while it is alive; retain the maximum observed WorkingSetSize and PrivateUsage.'
  revisionProvenance = 'Source and tested-tree revisions are supplied by the operator; binary/compiler/source/input hashes identify bytes but are not a signed build attestation.'
  memoryStatus = 'sampled-lower-bound'
  runs = @($sampleRuns)
  summary = [ordered]@{
    runCount = $Runs
    medianSampledPeakWorkingSetBytes = $workingSetValues[$medianIndex]
    medianSampledPeakPrivateBytes = $privateValues[$medianIndex]
    minimumLiveMemorySamples = [int](($sampleRuns | Measure-Object -Property liveMemorySamples -Minimum).Minimum)
  }
  limitations = @(
    'One Windows host and one native workflow binary; samples can miss brief peaks and are lower bounds.',
    'Values include process and runtime overhead, are not allocation counts or leak checks, and do not include compiler build memory.',
    'This profiler adds monitoring overhead, so it omits elapsed-time results and makes no cross-platform or general performance claim.',
    'The supplied revision IDs do not prove which compiler built the native binary; the report records and checks the available byte hashes separately.'
  )
}

$json = ConvertTo-Json -InputObject $profile -Depth 8
if ($OutputPath) {
  $absoluteOutputPath = [IO.Path]::GetFullPath($OutputPath)
  $outputDirectory = [IO.Path]::GetDirectoryName($absoluteOutputPath)
  if (-not (Test-Path -LiteralPath $outputDirectory -PathType Container)) {
    [IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
  }
  [IO.File]::WriteAllText($absoluteOutputPath, "$json" + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}
Write-Output $json