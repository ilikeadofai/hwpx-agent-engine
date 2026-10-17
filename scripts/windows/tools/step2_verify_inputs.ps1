# step2_verify_inputs.ps1
# 묶음 최상위의 SHA256SUMS.txt 와 실제 파일의 SHA-256을 대조한다. (읽기 전용)
# 형식: "<sha256>  <상대경로>" (sha256sum 형식, 구분자 '/', UTF-8)
# return\ 아래 파일은 사용자가 작성/수정하므로 해시가 달라도 실패로 세지 않고 '변경(정상)'으로 표시한다.
# Windows PowerShell 5.1 호환. 파일은 UTF-8 BOM + CRLF.

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

$exitCode = 0
try {
    $bundleRoot = Split-Path -Parent $PSScriptRoot
    $sumsPath = Join-Path $bundleRoot 'SHA256SUMS.txt'
    if (-not (Test-Path -LiteralPath $sumsPath)) {
        throw ('SHA256SUMS.txt 를 찾지 못했습니다. 묶음 최상위 폴더(' + $bundleRoot + ')에 있어야 합니다. 압축을 완전히 풀었는지 확인해 주세요.')
    }

    $rawLines = @(Get-Content -LiteralPath $sumsPath -Encoding UTF8)
    $countOk = 0
    $countBad = 0
    $countMissing = 0
    $countChanged = 0
    $countSkipped = 0

    Write-Host ''
    Write-Host '===== 입력 파일 확인 =====' -ForegroundColor Cyan
    Write-Host ('묶음 위치: ' + $bundleRoot)
    Write-Host ''

    foreach ($raw in $rawLines) {
        $line = ([string]$raw).TrimEnd("`r", "`n")
        if ($line.Trim() -eq '') { continue }
        if ($line.StartsWith('#')) { continue }

        # "<64자리 hex><공백><공백 또는 *><경로>"
        if ($line -notmatch '^([0-9a-fA-F]{64}) [ *](.+)$') {
            $countSkipped++
            Write-Host ('[건너뜀] 형식을 해석하지 못한 줄: ' + $line) -ForegroundColor Yellow
            continue
        }
        $expected = $Matches[1].ToLowerInvariant()
        $rel = $Matches[2]
        $relWin = $rel.Replace('/', '\')

        if ($relWin.StartsWith('\') -or $relWin -match '(^|\\)\.\.(\\|$)' -or $relWin -match '^[A-Za-z]:') {
            $countBad++
            Write-Host ('[불일치] 허용되지 않는 경로: ' + $rel) -ForegroundColor Red
            continue
        }

        $full = Join-Path $bundleRoot $relWin
        if (-not (Test-Path -LiteralPath $full -PathType Leaf)) {
            $countMissing++
            Write-Host ('[없음]   ' + $rel) -ForegroundColor Red
            continue
        }

        try {
            $actual = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToLowerInvariant()
        } catch {
            $countBad++
            Write-Host ('[불일치] 읽지 못함(다른 프로그램이 열고 있을 수 있음): ' + $rel) -ForegroundColor Red
            continue
        }

        if ($actual -eq $expected) {
            $countOk++
            Write-Host ('[OK]     ' + $rel) -ForegroundColor Green
        } elseif ($rel.StartsWith('return/')) {
            $countChanged++
            Write-Host ('[변경]   ' + $rel + '  (작성 중인 결과 파일이면 정상)') -ForegroundColor Yellow
        } else {
            $countBad++
            Write-Host ('[불일치] ' + $rel) -ForegroundColor Red
        }
    }

    Write-Host ''
    Write-Host '----- 요약 -----'
    Write-Host ('OK: ' + $countOk + ' / 불일치: ' + $countBad + ' / 없음: ' + $countMissing + ' / 변경(return): ' + $countChanged + ' / 건너뜀: ' + $countSkipped)
    if ($countBad -eq 0 -and $countMissing -eq 0 -and $countSkipped -eq 0) {
        Write-Host '결과: 모든 입력 파일이 일치합니다.' -ForegroundColor Green
    } else {
        $exitCode = 1
        Write-Host '결과: 문제가 있습니다. 불일치/없음 파일이 있으면 묶음을 다시 받아 압축을 푼 뒤 다시 확인해 주세요.' -ForegroundColor Red
        Write-Host '(입력 HWPX를 한글에서 열어 저장하지 않았는지도 확인해 주세요. 저장은 return\resaved 에 다른 이름으로 합니다.)'
    }
} catch {
    $exitCode = 2
    Write-Host ''
    Write-Host '[오류] 입력 확인 중 문제가 생겼습니다.' -ForegroundColor Red
    Write-Host ('원인: ' + $_.Exception.Message)
}
Write-Host ('종료 코드: ' + $exitCode)
exit $exitCode
