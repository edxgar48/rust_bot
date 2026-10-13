<#
.SYNOPSIS
  O que o Agendador de Tarefas executa: roda o `sync` (padrao: ciclo todas --aguardar).

.DESCRIPTION
  Entra na pasta de trabalho, liga o SSH sem perguntas (so para este processo),
  chama o binario release diretamente (sem cargo run) e devolve o codigo de
  saida do sync. Grava em <Pasta>\logs\agendador.log uma linha de inicio (antes
  do sync) e uma de fim; inicio sem fim = processo morto (ex.: limite de 3 h).
  Binario ou pasta ausentes: erro claro e saida diferente de zero.

  Windows PowerShell 5.1. Arquivo so em ASCII de proposito (o 5.1 le UTF-8 sem
  BOM como ANSI e estragaria os acentos).

.PARAMETER Pasta
  Pasta de trabalho (fixa sync.lock, logs/ e os bancos). Padrao: raiz do
  repositorio (pai de scripts/).

.PARAMETER Binario
  Executavel do sync. Padrao: <raiz do repositorio>\target\release\sync.exe.

.PARAMETER Argumentos
  Argumentos do sync. A string e dividida por espacos: nao ha suporte a
  argumento que contenha espaco. Padrao: "ciclo todas --aguardar".
#>
param(
    [string]$Pasta,
    [string]$Binario,
    [string]$Argumentos = 'ciclo todas --aguardar'
)

$raizRepo = Split-Path -Parent $PSScriptRoot
if (-not $Pasta) { $Pasta = $raizRepo }
if (-not $Binario) { $Binario = Join-Path $raizRepo 'target\release\sync.exe' }

$inicio = Get-Date

# Falha ao gravar o log nunca derruba a execucao: so avisa na tela.
function Gravar-Agendador([string]$pastaLog, [string]$texto) {
    try {
        $dir = Join-Path $pastaLog 'logs'
        if (-not (Test-Path -LiteralPath $dir)) {
            New-Item -ItemType Directory -Path $dir -Force | Out-Null
        }
        # AppendAllText com UTF8 sem BOM: o Add-Content -Encoding UTF8 do 5.1 poe BOM.
        # O .NET resolve caminho relativo pelo diretorio do processo, nao pelo do PS.
        $arq = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath((Join-Path $dir 'agendador.log'))
        [System.IO.File]::AppendAllText($arq, $texto + [Environment]::NewLine, (New-Object System.Text.UTF8Encoding($false)))
    } catch {
        [Console]::Error.WriteLine("AVISO: nao foi possivel gravar logs\agendador.log: $($_.Exception.Message)")
    }
}

function Finalizar([int]$codigo, [string]$detalhe) {
    $fim = Get-Date
    $linha = '{0} -> {1} | exit {2} | {3} | pasta={4} | binario={5} | args={6}' -f `
        $inicio.ToString('yyyy-MM-dd HH:mm:ss'), $fim.ToString('HH:mm:ss'), $codigo, $detalhe, $Pasta, $Binario, $Argumentos
    # Pasta ausente: o log cai na raiz do repositorio, para ainda deixar rastro.
    $destino = $Pasta
    if (-not (Test-Path -LiteralPath $Pasta -PathType Container)) { $destino = $raizRepo }
    Gravar-Agendador $destino $linha
    exit $codigo
}

if (-not (Test-Path -LiteralPath $Pasta -PathType Container)) {
    [Console]::Error.WriteLine("ERRO: pasta de trabalho nao existe: $Pasta")
    Finalizar 2 'ERRO pasta ausente'
}
if (-not (Test-Path -LiteralPath $Binario -PathType Leaf)) {
    [Console]::Error.WriteLine("ERRO: binario nao encontrado: $Binario (rode cargo build --release)")
    Finalizar 2 'ERRO binario ausente'
}

# Linha de inicio: se o Agendador matar o processo (limite de 3 h), sobra rastro.
Gravar-Agendador $Pasta ('{0} inicio | pasta={1} | binario={2} | args={3}' -f `
    $inicio.ToString('yyyy-MM-dd HH:mm:ss'), $Pasta, $Binario, $Argumentos)

try {
    Set-Location -LiteralPath $Pasta -ErrorAction Stop
    # Sem console, o ssh do `publicar` travaria pedindo senha ou confirmacao de host.
    $env:GIT_SSH_COMMAND = 'ssh -o BatchMode=yes -o ConnectTimeout=30'
    $lista = @($Argumentos -split '\s+' | Where-Object { $_ })
    & $Binario @lista
    $codigo = $LASTEXITCODE
    if ($null -eq $codigo) { $codigo = 1 }
} catch {
    [Console]::Error.WriteLine("ERRO: falha ao executar o binario: $($_.Exception.Message)")
    Finalizar 1 'ERRO ao executar'
}

if ($codigo -eq 0) { Finalizar 0 'sucesso' } else { Finalizar $codigo 'falha do sync' }
