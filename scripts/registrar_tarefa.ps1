<#
.SYNOPSIS
  Cria ou atualiza (sem duplicar) a tarefa agendada "rust_spider ciclo".

.DESCRIPTION
  Roda ciclo_agendado.ps1 todo dia as 23:00. Quem registra e o usuario, rodando
  este script a mao. Nao e chamado por nenhum teste, exceto com -Mostrar.

  -Mostrar  so imprime a definicao; nao toca no Agendador (sem privilegios).
  -Remover  apaga a tarefa, se existir.
  -SoConectado  roda so com o usuario conectado (LogonType Interactive): nao
            exige PowerShell elevado, mas nao roda as 23:00 se ninguem estiver
            logado (StartWhenAvailable roda no proximo login). Use quando o
            registro com S4U der "Acesso negado".

  Windows PowerShell 5.1. Arquivo so em ASCII de proposito.
#>
param(
    [string]$Pasta,
    [switch]$Remover,
    [switch]$Mostrar,
    [switch]$SoConectado
)

$nomeTarefa = 'rust_spider ciclo'
$raizRepo = Split-Path -Parent $PSScriptRoot
if (-not $Pasta) { $Pasta = $raizRepo }
# Caminho absoluto e sem barra final: `-Pasta "...\"` escaparia a aspa de fechamento
# e caminho relativo nao valeria no Agendador. Pasta inexistente = erro (exit 1).
# Raiz de unidade (C:\) e recusada: a barra final nao pode ser tirada.
$Pasta = (Resolve-Path -LiteralPath $Pasta -ErrorAction Stop).ProviderPath
if ($Pasta -match '^[A-Za-z]:\\?$') {
    [Console]::Error.WriteLine("ERRO: -Pasta nao pode ser a raiz de uma unidade ($Pasta); use uma subpasta.")
    exit 2
}
$Pasta = $Pasta.TrimEnd('\')
$scriptCiclo = Join-Path $PSScriptRoot 'ciclo_agendado.ps1'
$binario = Join-Path $raizRepo 'target\release\sync.exe'
$usuario = "$env:USERDOMAIN\$env:USERNAME"
$argumentoAcao = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$scriptCiclo`" -Pasta `"$Pasta`""
if ($SoConectado) {
    # Na sessao do usuario a janela do PowerShell apareceria: abre oculta.
    $argumentoAcao = "-WindowStyle Hidden $argumentoAcao"
    $tipoLogon = 'Interactive'
    $descricaoConta = 'LogonType Interactive (so com o usuario conectado; nao exige elevacao)'
} else {
    $tipoLogon = 'S4U'
    $descricaoConta = 'LogonType S4U (conectado ou nao, sem senha; pode exigir PowerShell elevado)'
}

if ($Mostrar) {
    Write-Output "Tarefa: $nomeTarefa (nada foi registrado)"
    Write-Output "Gatilho: diariamente as 23:00"
    Write-Output "Acao: powershell.exe $argumentoAcao"
    Write-Output "Comando do sync: ciclo todas --aguardar (binario $binario)"
    Write-Output "Conta: $usuario, $descricaoConta, RunLevel Limited"
    Write-Output "Configuracoes: StartWhenAvailable; sem WakeToRun; MultipleInstances IgnoreNew; ExecutionTimeLimit 3h; AllowStartIfOnBatteries; DontStopIfGoingOnBatteries"
    Write-Output "Pasta de trabalho: $Pasta"
    if (-not (Test-Path -LiteralPath $binario -PathType Leaf)) {
        Write-Output "AVISO: binario release nao existe ($binario); rode cargo build --release"
    }
    exit 0
}

$ErrorActionPreference = 'Stop'

if ($Remover) {
    if (Get-ScheduledTask -TaskName $nomeTarefa -ErrorAction SilentlyContinue) {
        Unregister-ScheduledTask -TaskName $nomeTarefa -Confirm:$false
        Write-Output "Tarefa '$nomeTarefa' removida."
    } else {
        Write-Output "Tarefa '$nomeTarefa' nao existe; nada a remover."
    }
    exit 0
}

if (-not (Test-Path -LiteralPath $binario -PathType Leaf)) {
    Write-Warning "binario release nao existe ($binario); rode cargo build --release antes da primeira execucao"
}

$acao = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $argumentoAcao -WorkingDirectory $Pasta
$gatilho = New-ScheduledTaskTrigger -Daily -At '23:00'
$conta = New-ScheduledTaskPrincipal -UserId $usuario -LogonType $tipoLogon -RunLevel Limited
$config = New-ScheduledTaskSettingsSet -StartWhenAvailable -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Hours 3) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries

# -Force atualiza a tarefa de mesmo nome em vez de duplicar.
Register-ScheduledTask -TaskName $nomeTarefa -Action $acao -Trigger $gatilho -Principal $conta `
    -Settings $config -Description 'sync ciclo todas --aguardar (rust_spider)' -Force | Out-Null
Write-Output "Tarefa '$nomeTarefa' registrada (diaria, 23:00, pasta $Pasta, LogonType $tipoLogon)."
Write-Output "Confira com: Get-ScheduledTaskInfo -TaskName '$nomeTarefa'"
