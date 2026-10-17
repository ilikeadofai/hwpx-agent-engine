# step1_environment.ps1
# Windows 환경 정보를 return\environment_auto.txt 에 기록한다. (읽기 전용 조회, 네트워크/레지스트리 쓰기/관리자 권한 없음)
# 기록하지 않는 것: 컴퓨터 이름, 사용자 이름, 제품키/ProductID, 설치 경로.
# Windows PowerShell 5.1 호환. 파일은 UTF-8 BOM + CRLF.

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

$exitCode = 0
try {
    $bundleRoot = Split-Path -Parent $PSScriptRoot
    $returnDir = Join-Path $bundleRoot 'return'
    if (-not (Test-Path -LiteralPath $returnDir)) {
        New-Item -ItemType Directory -Path $returnDir | Out-Null
    }
    $outPath = Join-Path $returnDir 'environment_auto.txt'

    $lines = New-Object System.Collections.ArrayList
    function Add-Line([string]$text) {
        [void]$lines.Add($text)
    }

    Add-Line '# 자동 기록 (step1_environment.ps1). 컴퓨터 이름, 사용자 이름, 제품키, 설치 경로는 기록하지 않음.'
    Add-Line ('생성 시각: ' + (Get-Date -Format 'yyyy-MM-dd HH:mm:ss zzz'))
    Add-Line ''

    # ---- Windows ----
    Add-Line '[Windows]'
    try {
        $os = Get-CimInstance -ClassName Win32_OperatingSystem
        Add-Line ('제품명: ' + $os.Caption)
        Add-Line ('버전: ' + $os.Version)
        Add-Line ('빌드: ' + $os.BuildNumber)
        Add-Line ('아키텍처: ' + $os.OSArchitecture)
    } catch {
        Add-Line '확인불가: Windows 정보를 읽지 못함'
    }
    Add-Line ''

    # ---- 한컴 제품 ----
    Add-Line '[한컴 제품 (설치된 프로그램 목록: 이름/버전/게시자만)]'
    $uninstallRoots = @(
        'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKCU:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    $hancomFound = @{}
    foreach ($root in $uninstallRoots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $keys = @(Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue)
        foreach ($k in $keys) {
            try {
                $name = $k.GetValue('DisplayName')
                if ($null -eq $name) { continue }
                $name = [string]$name
                if ($name -notmatch '한글|Hancom|HWP|한컴') { continue }
                $ver = $k.GetValue('DisplayVersion')
                $pub = $k.GetValue('Publisher')
                if ($null -eq $ver) { $ver = '' }
                if ($null -eq $pub) { $pub = '' }
                $entry = ('이름=' + $name + ' | 버전=' + [string]$ver + ' | 게시자=' + [string]$pub)
                $hancomFound[$entry] = $true
            } catch {
                # 개별 키를 읽지 못해도 계속한다.
            }
        }
    }
    if ($hancomFound.Count -eq 0) {
        Add-Line '한컴 제품 항목을 찾지 못함 (설치 목록에 없거나 이름이 다름). 한글 [도움말 > 한글 정보]의 버전을 직접 적을 것.'
    } else {
        foreach ($e in ($hancomFound.Keys | Sort-Object)) {
            Add-Line $e
        }
    }
    Add-Line ''

    # ---- 글꼴 ----
    Add-Line '[글꼴 설치 여부 (레지스트리 글꼴 목록 + 설치 글꼴 목록 기준)]'
    $fontValueNames = New-Object System.Collections.ArrayList
    foreach ($fp in @('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts', 'HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts')) {
        try {
            if (Test-Path -LiteralPath $fp) {
                $fk = Get-Item -LiteralPath $fp
                foreach ($vn in $fk.GetValueNames()) { [void]$fontValueNames.Add([string]$vn) }
            }
        } catch {
            # 무시
        }
    }
    try {
        Add-Type -AssemblyName System.Drawing
        $fc = New-Object System.Drawing.Text.InstalledFontCollection
        foreach ($fam in $fc.Families) { [void]$fontValueNames.Add([string]$fam.Name) }
    } catch {
        # 무시
    }

    # 이름 패턴 (한글/영문 이름 모두). '바탕'은 '함초롬바탕'과 구분하기 위해 앞쪽을 고정한다.
    $fontChecks = @(
        @('함초롬바탕', '^(함초롬바탕|HCR\s*Batang)'),
        @('함초롬돋움', '^(함초롬돋움|HCR\s*Dotum)'),
        @('나눔고딕', '^(나눔고딕|Nanum\s*Gothic)'),
        @('맑은 고딕', '^(맑은 고딕|Malgun Gothic)'),
        @('바탕', '^(바탕|Batang)(\s|\(|&|$)'),
        @('굴림', '^(굴림|Gulim)(\s|\(|&|$)')
    )
    foreach ($fcItem in $fontChecks) {
        $label = $fcItem[0]
        $pattern = $fcItem[1]
        $hit = $false
        foreach ($n in $fontValueNames) {
            if ($n -match $pattern) { $hit = $true; break }
        }
        if ($hit) { Add-Line ($label + ': 있음') } else { Add-Line ($label + ': 없음') }
    }
    Add-Line ''

    # ---- 기본 프린터 / 용지 ----
    Add-Line '[기본 프린터]'
    $printerName = $null
    try {
        $printers = @(Get-CimInstance -ClassName Win32_Printer | Where-Object { $_.Default -eq $true })
        if ($printers.Count -gt 0) { $printerName = [string]$printers[0].Name }
    } catch {
        $printerName = $null
    }
    if ($null -eq $printerName -or $printerName -eq '') {
        Add-Line '기본 프린터: 확인불가'
        Add-Line '기본 용지: 확인불가'
    } else {
        # 괄호 안 장치 식별자(시리얼/MAC 유사값)는 기록하지 않는다.
        $printerLabel = $printerName -replace '\s*\([^()]*\)', ''
        Add-Line ('기본 프린터: ' + $printerLabel)
        $paper = '확인불가'
        try {
            Add-Type -AssemblyName System.Drawing
            $ps = New-Object System.Drawing.Printing.PrinterSettings
            $ps.PrinterName = $printerName
            if ($ps.IsValid) {
                $pn = $ps.DefaultPageSettings.PaperSize.PaperName
                if ($null -ne $pn -and $pn -ne '') { $paper = [string]$pn }
            }
        } catch {
            $paper = '확인불가'
        }
        Add-Line ('기본 용지: ' + $paper)
    }
    Add-Line ''

    # ---- 화면 배율 (선택) ----
    Add-Line '[화면 배율 (선택)]'
    $scale = '확인불가'
    try {
        $dpiKey = 'HKCU:\Control Panel\Desktop\WindowMetrics'
        if (Test-Path -LiteralPath $dpiKey) {
            $dk = Get-Item -LiteralPath $dpiKey
            $applied = $dk.GetValue('AppliedDPI')
            if ($null -ne $applied) {
                $scale = ([string][math]::Round(([double]$applied) * 100 / 96)) + '% (AppliedDPI=' + [string]$applied + ')'
            }
        }
    } catch {
        $scale = '확인불가'
    }
    Add-Line ('배율: ' + $scale)

    $text = ($lines -join "`r`n") + "`r`n"
    $utf8Bom = New-Object System.Text.UTF8Encoding($true)
    [System.IO.File]::WriteAllText($outPath, $text, $utf8Bom)

    Write-Host ''
    Write-Host '===== 환경 기록 요약 =====' -ForegroundColor Cyan
    foreach ($l in $lines) { Write-Host $l }
    Write-Host ''
    Write-Host ('저장 위치: ' + $outPath) -ForegroundColor Green
    Write-Host '참고: return\environment.txt 의 항목(폰트 대체 알림 등)은 직접 적어야 합니다.'
} catch {
    $exitCode = 1
    Write-Host ''
    Write-Host '[오류] 환경 기록 중 문제가 생겼습니다.' -ForegroundColor Red
    Write-Host ('원인: ' + $_.Exception.Message)
    Write-Host '이 창의 내용을 그대로 복사해 전달해 주세요. return\environment.txt 는 직접 적어도 됩니다.'
}
exit $exitCode
