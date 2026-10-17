# step3_pack_results.ps1
# return\ 아래 파일의 SHA-256을 return\RETURN_SHA256SUMS.txt 에 기록하고, return 폴더를 바탕화면에 zip으로 묶는다.
# 입력 파일은 수정하지 않는다. 기존 zip을 덮어쓰지 않는다.
# RETURN_SHA256SUMS.txt 형식: "<sha256>  <return 기준 상대경로>" (sha256sum 형식, '/' 구분, UTF-8 BOM 없음, LF)
# Windows PowerShell 5.1 호환. 파일은 UTF-8 BOM + CRLF.

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

$exitCode = 0
try {
    $bundleRoot = Split-Path -Parent $PSScriptRoot
    $returnDir = Join-Path $bundleRoot 'return'
    if (-not (Test-Path -LiteralPath $returnDir -PathType Container)) {
        throw ('return 폴더를 찾지 못했습니다: ' + $returnDir)
    }
    $returnFull = (Get-Item -LiteralPath $returnDir).FullName.TrimEnd('\')
    $sumsName = 'RETURN_SHA256SUMS.txt'
    $sumsPath = Join-Path $returnDir $sumsName

    Write-Host ''
    Write-Host '===== 결과 묶기 =====' -ForegroundColor Cyan

    # ---- 경고 점검 (실패 아님) ----
    $warnings = New-Object System.Collections.ArrayList

    $resultsPath = Join-Path $returnDir 'results.csv'
    if (-not (Test-Path -LiteralPath $resultsPath -PathType Leaf)) {
        [void]$warnings.Add('results.csv 가 없습니다.')
    } else {
        try {
            $rows = @(Import-Csv -LiteralPath $resultsPath -Encoding UTF8)
            $emptyRows = 0
            foreach ($row in $rows) {
                $props = @($row.PSObject.Properties)
                $filled = $false
                foreach ($p in $props) {
                    if ($p.Name -eq '파일') { continue }
                    if ($null -ne $p.Value -and ([string]$p.Value).Trim() -ne '') { $filled = $true; break }
                }
                if (-not $filled) { $emptyRows++ }
            }
            if ($emptyRows -gt 0) {
                [void]$warnings.Add('results.csv 에서 ''파일'' 외 칸이 모두 빈 행이 ' + $emptyRows + '개 / 전체 ' + $rows.Count + '행 있습니다. (확인하지 않은 파일은 ''미확인''으로 적어 주세요.)')
            }
        } catch {
            [void]$warnings.Add('results.csv 를 읽지 못했습니다: ' + $_.Exception.Message)
        }
    }

    $envPath = Join-Path $returnDir 'environment.txt'
    if (-not (Test-Path -LiteralPath $envPath -PathType Leaf)) {
        [void]$warnings.Add('environment.txt 가 없습니다.')
    } else {
        $envLines = @(Get-Content -LiteralPath $envPath -Encoding UTF8)
        $valueCount = 0
        foreach ($el in $envLines) {
            $t = [string]$el
            $idx = $t.IndexOfAny([char[]]@(':', [char]0xFF1A))
            if ($idx -ge 0) {
                if ($t.Substring($idx + 1).Trim() -ne '') { $valueCount++ }
            } elseif ($t.Trim() -ne '') {
                $valueCount++
            }
        }
        if ($valueCount -eq 0) {
            [void]$warnings.Add('environment.txt 가 비어 있습니다. (항목 이름만 있고 값이 없음)')
        }
    }

    # ---- 해시 기록 ----
    $files = @(Get-ChildItem -LiteralPath $returnDir -Recurse -File | Where-Object {
        -not ($_.Name -eq $sumsName -and $_.DirectoryName -eq $returnFull)
    })
    $entries = New-Object System.Collections.ArrayList
    foreach ($f in $files) {
        $relPath = $f.FullName.Substring($returnFull.Length + 1).Replace('\', '/')
        try {
            $hash = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        } catch {
            throw ('파일을 읽지 못했습니다: ' + $relPath + ' (한글/엑셀 등에서 열려 있으면 모두 닫고 다시 실행해 주세요.)')
        }
        [void]$entries.Add([pscustomobject]@{ Rel = $relPath; Hash = $hash })
    }
    $sorted = @($entries | Sort-Object -Property Rel)
    $sumLines = New-Object System.Collections.ArrayList
    foreach ($e in $sorted) {
        [void]$sumLines.Add($e.Hash + '  ' + $e.Rel)
    }
    $sumText = ''
    if ($sumLines.Count -gt 0) { $sumText = ($sumLines -join "`n") + "`n" }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($sumsPath, $sumText, $utf8NoBom)
    Write-Host ('해시 기록: ' + $sumLines.Count + '개 파일 -> ' + $sumsPath)

    if ($files.Count -eq 0) {
        [void]$warnings.Add('return 폴더에 파일이 없습니다.')
    }

    foreach ($w in $warnings) {
        Write-Host ('[경고] ' + $w) -ForegroundColor Yellow
    }

    # ---- zip 만들기 ----
    $desktop = [Environment]::GetFolderPath('Desktop')
    if ([string]::IsNullOrEmpty($desktop) -or -not (Test-Path -LiteralPath $desktop)) {
        throw '바탕화면 폴더를 찾지 못했습니다.'
    }
    $stamp = Get-Date -Format 'yyyyMMdd_HHmm'
    $zipPath = Join-Path $desktop ('Windows_결과_' + $stamp + '.zip')
    $n = 2
    while (Test-Path -LiteralPath $zipPath) {
        $zipPath = Join-Path $desktop ('Windows_결과_' + $stamp + '_' + $n + '.zip')
        $n++
    }

    try {
        Compress-Archive -LiteralPath $returnDir -DestinationPath $zipPath
    } catch {
        throw ('zip 만들기에 실패했습니다. 한글에서 return 폴더의 파일을 열어 둔 채라면 한글을 모두 닫고 다시 실행해 주세요. (' + $_.Exception.Message + ')')
    }

    Write-Host ''
    Write-Host '완료. 아래 파일을 전달해 주세요.' -ForegroundColor Green
    Write-Host $zipPath
    if ($warnings.Count -gt 0) {
        Write-Host ('경고 ' + $warnings.Count + '건이 있었습니다. 위 경고를 확인해 주세요. (zip은 만들어졌습니다.)') -ForegroundColor Yellow
    }
} catch {
    $exitCode = 1
    Write-Host ''
    Write-Host '[오류] 결과 묶기 중 문제가 생겼습니다.' -ForegroundColor Red
    Write-Host ('원인: ' + $_.Exception.Message)
}
exit $exitCode
