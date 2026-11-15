#requires -Version 5.1
# Integration test: CLI must write a Failed JSON verdict AND return nonzero.
# All inputs are a tiny temporary fake cache. No Java, Forge or Minecraft data.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ("tl03-failed-cli-" + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($root)
try {
  $profile = Join-Path $PSScriptRoot 'fixtures/forge_36229_official_profile.json'
  $version = Join-Path $PSScriptRoot 'fixtures/forge_36229_official_version.json'
  $config = Join-Path $root 'config.json'
  $report = Join-Path $root 'report.json'
  $obj = @{
    cache_roots = @($root)
    client = @{
      path = (Join-Path $root 'not-present-client.jar')
      url = 'https://unapproved.invalid/v1/objects/37fd3c903861eeff3bc24b71eed48f828b5269c8/client.jar'
      sha1 = '37fd3c903861eeff3bc24b71eed48f828b5269c8'
      size_bytes = 17547153
    }
    binpatch = @{
      installer = (Join-Path $root 'forge-1.16.5-36.2.29-installer.jar')
      installer_sha1 = '7d247279cf06eed6ffcf609f36d11e34c70350f3'
      entry = 'data/client.lzma'
      extracted = (Join-Path $root 'client.lzma')
      extracted_sha1 = '6b494c622457566e6db2bed43a2de4a735ac72a6'
      report = (Join-Path $root 'binpatch-provenance.json')
    }
    java8 = (Join-Path $root 'nonexistent-java8.exe')
    stage_root = (Join-Path $root 'never-created-stage')
    timeout_seconds = 120
  }
  [IO.File]::WriteAllText($config,($obj|ConvertTo-Json -Depth 12),(New-Object Text.UTF8Encoding($false)))
  & cargo.exe run --quiet --locked -p tl-loader --example forge_cache_preflight -- $profile $version $config $report | Out-Null
  $exit = $LASTEXITCODE
  if ($exit -eq 0 -or $exit -ne 2) { throw "FAILED CLI must exit 2, got $exit" }
  if (-not (Test-Path -LiteralPath $report -PathType Leaf)) { throw 'FAILED CLI did not preserve JSON report' }
  $result = Get-Content -LiteralPath $report -Raw | ConvertFrom-Json
  if ($result.import.state -ne 'Failed' -or $result.prepare.state -ne 'Failed' -or
      $result.prepare.processor_plan_count -ne 0 -or $result.prepare.java_executed -ne $false -or
      $result.prepare.instance_ready -ne $false -or (Test-Path -LiteralPath $obj.stage_root)) {
      throw 'FAILED CLI returned inconsistent JSON or created stage'
  }
  Write-Output 'FORGE_FAILED_CLI_NONZERO_AND_JSON_VERDICT_PASS_NO_JAVA'
}
finally { [IO.Directory]::Delete($root,$true) }
