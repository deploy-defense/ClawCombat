# PowerShell 세션 및 출력 인코딩을 UTF-8로 강제 고정
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# 제외할 폴더 목록
$excludeDirs = @("target", "node_modules", ".git", "llm_agent", "oc_launcher", "oc_core", "examples", "crates", "battle_tools")

# 파일 수집 및 합치기 (-Filter 대신 -Include를 사용하여 여러 패턴 지정)
$result = Get-ChildItem -Recurse -Include "*.rs", "Cargo.toml" | Where-Object {
    $path = $_.FullName
    $exclude = $false
    foreach ($dir in $excludeDirs) {
        if ($path -like "*\$dir\*") { $exclude = $true; break }
    }
    -not $exclude
} | ForEach-Object {
    "========================================="
    "File: $($_.FullName)"
    "-----------------------------------------"
    # Cargo.toml과 Rust 파일 모두 UTF-8 기반이므로 명시적 읽기
    Get-Content -Path $_.FullName -Raw -Encoding UTF8
    ""
}

# .NET 기능을 직접 호출하여 완벽한 UTF-8로 파일에 기록
[System.IO.File]::WriteAllLines((Join-Path (Get-Location) "full.txt"), $result, [System.Text.Encoding]::UTF8)

Write-Host "완료!" -ForegroundColor Green