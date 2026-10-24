# Resolve only the configured SDK; all build orchestration lives there.
$ErrorActionPreference='Stop'
if(@($args | Where-Object { $_ -ceq 'rust-native-test' }).Count){
    if($args.Count -ne 1){throw 'rust-native-test accepts no additional build arguments'}
    & (Join-Path $PSScriptRoot 'Tests/RustNative.ps1')
    exit $LASTEXITCODE
}
$sdkSetting=$null
foreach($line in Get-Content -LiteralPath (Join-Path $PSScriptRoot 'Settings.R4S')){
    if($line -match '^SDK_ROOT=(.+)$'){$sdkSetting=$Matches[1]}
}
if(!$sdkSetting){throw 'Missing SDK_ROOT in Settings.R4S'}
$sdkPath=[IO.Path]::GetFullPath($sdkSetting.Replace('\',[IO.Path]::DirectorySeparatorChar),$PSScriptRoot)
& (Join-Path $sdkPath 'Tools/BuildModule.ps1') -ModuleRoot $PSScriptRoot -BuildArguments @($args)
exit $LASTEXITCODE
