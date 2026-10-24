param([string]$Filter='')
# Current Rust Desktop owners against retained independent original oracles.
# General compiler selection and native GUI implementation remain in the SDK.
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$moduleRoot=Split-Path $PSScriptRoot -Parent
$workspaceRoot=[IO.Path]::GetFullPath('../../..',$moduleRoot)
$settings=@{}
foreach($line in Get-Content -LiteralPath (Join-Path $moduleRoot 'Settings.R4S')){
    if($line -match '^([A-Z_]+)=(.+)$'){$settings[$Matches[1]]=$Matches[2]}
}
function SettingPath([string]$Name,[string]$Base){
    if(!$settings.ContainsKey($Name)){throw ('Missing '+$Name+' in Desktop Settings.R4S')}
    [IO.Path]::GetFullPath($settings[$Name].Replace('\',[IO.Path]::DirectorySeparatorChar),$Base)
}
$sdk=SettingPath 'SDK_ROOT' $moduleRoot
$contract=SettingPath 'CONTRACT_ROOT' $moduleRoot
$libraries=SettingPath 'LIBRARIES_ROOT' $moduleRoot
$devkit=SettingPath 'DEVKIT_ROOT' $moduleRoot
$zigRoot=SettingPath 'ZIG_ROOT' $devkit
$suffix=if($IsWindows){'.exe'}else{''}
$zig=Join-Path $zigRoot ('zig'+$suffix)
$tools=if($settings.ContainsKey('RUST_TOOLS_ROOT')){SettingPath 'RUST_TOOLS_ROOT' $moduleRoot}else{Join-Path $workspaceRoot 'Artifacts/SDK/HostTools'}
$setup=Join-Path $tools 'Setup/Rust.ps1'
if(!(Test-Path -LiteralPath $setup -PathType Leaf)){throw 'The configured SDK Rust toolset is required for rust-native-test.'}
$pwsh=(Get-Process -Id $PID).Path
$descriptionText=& $pwsh -NoLogo -NoProfile -File $setup --root $devkit check
if($LASTEXITCODE -ne 0){throw 'SDK Rust installation check failed'}
$description=($descriptionText -join "`n")|ConvertFrom-Json
if(!$description.installed){throw ('The selected SDK Rust compiler is not installed: '+$description.reason)}
$rustc=Join-Path $description.toolchain ('bin/rustc'+$suffix)
$work=Join-Path $workspaceRoot ('Temp/Desktop-RustNative-'+[guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($work)|Out-Null
$receipt=Join-Path $workspaceRoot 'Temp/Desktop-RustNative.latest.json'
$success=$false
function InvokeChecked([string]$Program,[string[]]$Arguments){
    & $Program @Arguments
    if($LASTEXITCODE -ne 0){throw ('Desktop Rust native command failed: '+[IO.Path]::GetFileName($Program))}
}
try {
    $native=Join-Path $work 'native'
    Push-Location $sdk
    try {
        InvokeChecked $zig @('build',('--fork='+$contract),'-Dfacade-runtime-only=true','-Dfacade-host-test=true','--prefix',$native,'-j1')
        InvokeChecked $zig @('build',('--fork='+$contract),('-Dlibrary-helpers-root='+$libraries),'-Dlibrary-helpers-host-test=true','--prefix',$native,'-j1')
    }finally{Pop-Location}
    Push-Location $moduleRoot
    try {
        $imageSource=Join-Path $libraries 'R4IMG/Source'
        $imageOracle=Join-Path $work 'image-oracle'
        [IO.Directory]::CreateDirectory($imageOracle)|Out-Null
        Get-ChildItem -LiteralPath $imageSource -File -Filter '*.zig'|ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $imageOracle $_.Name)
        }
        $imageMain=Join-Path $imageOracle 'main.zig'
        $originalImage=[IO.File]::ReadAllText($imageMain)
        $entry='export fn r4l_entry() linksection(".text.r4l_entry") callconv(.c) void {}'
        if(($originalImage.Split($entry,[StringSplitOptions]::None)).Count -ne 2){throw 'Expected exactly one original R4IMG entry in native aggregate'}
        [IO.File]::WriteAllText($imageMain,$originalImage.Replace($entry,''),[Text.UTF8Encoding]::new($false))
        InvokeChecked $zig @('build','rust-provider-oracle','-Dhost-tests=true',('--fork='+$sdk),('--fork='+$contract),('--fork='+$libraries),('-Drust-image-oracle-root='+$imageOracle),'--prefix',$native,'-j1')
    }finally{Pop-Location}
    $facades=Join-Path $native 'lib/r4sdk_facades.o'
    $textOracle=Join-Path $work 'text-boundaries.o'
    InvokeChecked $zig @('cc','-O1','-fPIC','-Werror','-I',(Join-Path $sdk 'Shared/C/include'),'-I',(Join-Path $contract 'Generated/SDK/C/include'),'-c',(Join-Path $sdk 'Tests/Conformance/BoundedGraphemeChecks.c'),'-o',$textOracle)
    $external=Join-Path $libraries 'R4ACO/ThirdParty/Mesa26.2.2/Original/src/util/blake3'
    $cSources=@((Join-Path $moduleRoot 'src/rust/render/content_hash.c'))+@(foreach($name in @('blake3.c','blake3_portable.c','blake3_dispatch.c')){Join-Path $external $name})
    $object=Join-Path $work 'content.o'
    InvokeChecked $zig @('cc','-O2','-I',$external,'-c',$cSources[0],'-o',$object)
    $floatSource=Join-Path $moduleRoot 'src/rust/render/float_intrinsics.c'
    $floatObject=Join-Path $work 'float-intrinsics.o'
    InvokeChecked $zig @('cc','-O2','-fPIC','-Werror','-c',$floatSource,'-o',$floatObject)
    $objects=@($object,$floatObject)
    $archiveName=if($IsWindows){'desktop-content.lib'}else{'libdesktop-content.a'}
    InvokeChecked $zig (@('ar','rcs',(Join-Path $work $archiveName))+$objects)
    $geometry=Join-Path $libraries 'R4GFX/Display/Rust/src/lib.rs'
    $geometryLib=Join-Path $work 'libr4os_display_geometry.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4os_display_geometry','--crate-type','rlib',$geometry,'-o',$geometryLib)
    $textCore=Join-Path $libraries 'R4STD/Rust/lib.rs'
    $textLib=Join-Path $work 'libr4os_text_core.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4os_text_core','--crate-type','rlib',$textCore,'-o',$textLib)
    $geometryTest=Join-Path $work ('geometry-tests'+$suffix)
    InvokeChecked $rustc @('--edition=2024','--test',$geometry,'-o',$geometryTest)
    $geometryResult=& $geometryTest
    if($LASTEXITCODE -ne 0){$geometryResult|ForEach-Object {Write-Host $_};throw 'Original R4GFX geometry cases failed'}
    $contractSource=Join-Path $contract 'Generated/SDK/Rust/src/lib.rs'
    $contractLib=Join-Path $work 'libr4os_contract.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4os_contract','--crate-type','rlib',$contractSource,'-o',$contractLib)
    $sdkSource=Join-Path $sdk 'Rust/src/lib.rs'
    $sdkLib=Join-Path $work 'libr4os_sdk.rlib'
    # The freestanding SDK closes memcpy/memset inside this executable too.
    # Match its normal size optimization; debug byte helpers make the original
    # per-pixel GPU model spend minutes resetting otherwise empty resources.
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4os_sdk','--crate-type','rlib',$sdkSource,'--cfg','feature="alloc"','-C','opt-level=s','--extern',('r4os_contract='+$contractLib),'-L',('dependency='+$work),'-o',$sdkLib)
    $clockSource=Join-Path $sdk 'Tests/Simulation/PreciseClock.rs'
    $clockTest=Join-Path $work ('clock-tests'+$suffix)
    InvokeChecked $rustc @('--edition=2024','--test',$clockSource,'--extern',('r4os_sdk='+$sdkLib),'-L',('dependency='+$work),
        '-C',('link-arg='+(Join-Path $native 'lib/r4sdk_storage.o')),
        '-C',('link-arg='+$facades),'-o',$clockTest)
    $clockResult=& $clockTest
    if($LASTEXITCODE -ne 0){$clockResult|ForEach-Object{Write-Host $_};throw 'Existing precise clock source admission failed'}
    $librarySource=Join-Path $libraries 'Shared/Rust/src/lib.rs'
    $libraryLib=Join-Path $work 'libr4os_library_helpers.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4os_library_helpers','--crate-type','rlib',$librarySource,'--extern',('r4os_sdk='+$sdkLib),
        '--extern',('r4os_text_core='+$textLib),'-L',('dependency='+$work),'-o',$libraryLib)
    $graphicsSource=Join-Path $libraries 'R4GFX/Bindings/Rust/src/lib.rs'
    $graphicsLib=Join-Path $work 'libr4gfx.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4gfx','--crate-type','rlib',$graphicsSource,'--extern',('r4os_sdk='+$sdkLib),'-L',('dependency='+$work),'-o',$graphicsLib)
    $source=Join-Path $moduleRoot 'src/rust/lib.rs'
    $encoderSource=Join-Path $libraries 'R4ENC/Bindings/Rust/src/lib.rs'
    $encoderLib=Join-Path $work 'libr4enc.rlib'
    InvokeChecked $rustc @('--edition=2024','--crate-name','r4enc','--crate-type','rlib',$encoderSource,'--extern',('r4os_sdk='+$sdkLib),'-L',('dependency='+$work),'-o',$encoderLib)
    $binary=Join-Path $work ('desktop-tests'+$suffix)
    # Original freestanding ICC objects use absolute large-model relocations.
    # The Linux native fixture therefore links as an ordinary executable.
    $hostLinkArgs=if($IsLinux){@('-C','link-arg=-no-pie')}else{@()}
    InvokeChecked $rustc (@('--edition=2024','--test',$source,'--extern',('r4os_display_geometry='+$geometryLib),'--extern',('r4os_contract='+$contractLib),
        '--extern',('r4os_sdk='+$sdkLib),'--extern',('r4os_text_core='+$textLib),'--extern',('r4os_library_helpers='+$libraryLib),'--extern',('r4gfx='+$graphicsLib),
        '--extern',('r4enc='+$encoderLib),'-L',('dependency='+$work),'-L',('native='+$work),'-l','static=desktop-content','-C',('link-arg='+$facades),
        '-C',('link-arg='+(Join-Path $native 'lib/r4sdk_storage.o')),'-C',('link-arg='+$textOracle),
        '-C',('link-arg='+(Join-Path $native 'lib/r4libraries_helpers.o')),'-C',('link-arg='+(Join-Path $native 'lib/r4libraries_facades.o')),
        '-C',('link-arg='+(Join-Path $native 'lib/desktop-rust-standard.o')),'-o',$binary)+$hostLinkArgs)
    $selections=@(if($Filter){$Filter.Split(',')|ForEach-Object {$_.Trim()}}else{''})
    $desktopResult=@()
    foreach($selection in $selections){
        if($Filter -and $selection -cnotmatch '^[A-Za-z0-9_:]+$'){throw 'Use comma-separated literal Rust case scopes for -Filter.'}
        $part=if($selection){& $binary $selection --nocapture}else{& $binary}
        if($LASTEXITCODE -ne 0){$part|ForEach-Object {Write-Host $_};throw 'Desktop Rust owner cases failed'}
        if($selection -and ($part -join "`n") -notmatch 'test result: ok\. [1-9][0-9]* passed; 0 failed;'){throw ('Empty Desktop Rust selection: '+$selection)}
        $desktopResult+=@($part)
    }
    $allResults=(@($geometryResult)+@($desktopResult)) -join "`n"
    $caseCounts=@([regex]::Matches($allResults,'test result: ok\. (\d+) passed; 0 failed;')|ForEach-Object {[int]$_.Groups[1].Value})
    if($caseCounts.Count -ne (1+$selections.Count)){throw 'Missing native test summaries'}
    $desktopCases=[int](($caseCounts|Select-Object -Skip 1|Measure-Object -Sum).Sum)
    $inputs=@{}
    $files=@(Get-ChildItem -LiteralPath (Join-Path $moduleRoot 'src/rust') -Recurse -File -Filter '*.rs')+@(Get-ChildItem -LiteralPath (Join-Path $sdk 'Rust/src') -Recurse -File -Filter '*.rs')+@(Get-ChildItem -LiteralPath (Join-Path $libraries 'R4STD/Rust') -File -Filter '*.rs')
    $files+=@(Get-ChildItem -LiteralPath (Join-Path $libraries 'R4STD/Source') -Recurse -File -Filter '*.zig')+@(Get-ChildItem -LiteralPath (Join-Path $libraries 'Shared/Rust/src') -File -Filter '*.rs')+@(Get-ChildItem -LiteralPath (Join-Path $moduleRoot 'Tests/RustNative') -File -Filter '*.rs')
    $files+=@(Get-ChildItem -LiteralPath (Join-Path $moduleRoot 'Tests/Original') -File -Filter '*.zig')
    foreach($tree in @('R4GFX/Source','R4GFX/ThirdParty/LittleCMS/src','R4GFX/ThirdParty/LittleCMS/include','R4IMG/Source','R4IMG/ThirdParty/stb')){
        $files+=@(Get-ChildItem -LiteralPath (Join-Path $libraries $tree) -Recurse -File|Where-Object {$_.Extension -in @('.zig','.c','.h')})
    }
    $files+=@(Get-Item -LiteralPath (Join-Path $libraries 'R4IMG/Tests/Decoder/png_fixture.zig'),(Join-Path $libraries 'R4IMG/Tests/Decoder/raster_fixture.zig'),(Join-Path $libraries 'R4GFX/Tests/Color/fixtures.zig'))
    $paths=@($files.FullName)+@($PSCommandPath,$floatSource,$encoderSource,(Join-Path $libraries 'R4ENC/Bindings/Rust/src/helpers.rs'),(Join-Path $moduleRoot 'Build.ps1'),(Join-Path $moduleRoot 'build.zig'),(Join-Path $moduleRoot 'Tests/RustNative/standard_provider.zig'),
        $source,$geometry,$contractSource,$graphicsSource,(Join-Path $libraries 'R4GFX/color_build.zig'),(Join-Path $libraries 'R4GFX/Contract/Generated/implementation_abi.zig'),(Join-Path $libraries 'R4STD/Bindings/Rust/src/helpers.rs'),(Join-Path $libraries 'R4STD/Contract/Generated/implementation_abi.zig'),
        (Join-Path $sdk 'Inventory/LanguageParity.json'),(Join-Path $sdk 'Tests/Conformance/BoundedGraphemeChecks.c'),$clockSource,
        (Join-Path $sdk 'r4os/gui_value_bridge.zig'),(Join-Path $sdk 'r4os/data_value_bridge.zig'),(Join-Path $sdk 'Tests/Fixtures/Icons/windows32.ico'),(Join-Path $sdk 'Shared/C/include/r4os/storage_helpers.h'))+$cSources+@((Join-Path $external 'blake3.h'),(Join-Path $external 'blake3_impl.h'))
    $paths+=@('Application.ico','Terminal.ico'|ForEach-Object {Join-Path $workspaceRoot ('Repositories/Distribution/Injection/R4OS/Media/Icons/'+$_)})
    foreach($path in $paths|Sort-Object -Unique){$inputs[[IO.Path]::GetRelativePath($workspaceRoot,$path).Replace('\','/')]=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
    $observations=@($desktopResult|Where-Object {$_ -is [string] -and $_.StartsWith('{"schema":"r4os.desktop-work"')}|ForEach-Object {ConvertFrom-Json -InputObject $_ -AsHashtable})
    $proof=[ordered]@{schema=1;scope='current Desktop Rust owner/original comparison; native only';passed=$true;common_geometry_cases=$caseCounts[0];desktop_cases=$desktopCases;sdk_precise_clock_cases=1;filter=$Filter;observations=$observations;
        compiler=((& $rustc --version)-join '');compiler_sha256=(Get-FileHash -LiteralPath $rustc -Algorithm SHA256).Hash.ToLowerInvariant();
        zig_sha256=(Get-FileHash -LiteralPath $zig -Algorithm SHA256).Hash.ToLowerInvariant();toolset_manifest_sha256=$description.toolset_manifest_sha256;
        host=$description.host;sdk_optimization='s';productive_rust_entry=$true;inputs=$inputs;
        sdk_runtime_scope='normal shared SDK facade/storage runtime; native ABI; includes GUI/graphics/data/services';
        gui_native_sha256=(Get-FileHash -LiteralPath (Join-Path $native 'lib/r4sdk_storage.o') -Algorithm SHA256).Hash.ToLowerInvariant();
        sdk_facades_sha256=(Get-FileHash -LiteralPath $facades -Algorithm SHA256).Hash.ToLowerInvariant();
        library_native_sha256=(Get-FileHash -LiteralPath (Join-Path $native 'lib/r4libraries_helpers.o') -Algorithm SHA256).Hash.ToLowerInvariant();
        provider_native_sha256=(Get-FileHash -LiteralPath (Join-Path $native 'lib/desktop-rust-standard.o') -Algorithm SHA256).Hash.ToLowerInvariant()}
    [IO.File]::WriteAllText($receipt,($proof|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
    Write-Host ('R4GFX geometry: '+$caseCounts[0]+' passed; Desktop Rust owners: '+$desktopCases+' passed. Native only.')
    $success=$true
}finally{
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
    if(!$success){Remove-Item -LiteralPath $receipt -Force -ErrorAction SilentlyContinue}
}
exit 0
