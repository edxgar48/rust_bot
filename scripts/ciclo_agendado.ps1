<#
.SYNOPSIS
  O que o Agendador de Tarefas executa: roda o `sync` (padrao: ciclo todas --aguardar).

.DESCRIPTION
  Entra na pasta de trabalho, liga o SSH sem perguntas (so para este processo),
  chama o binario release diretamente (sem cargo run) e devolve o codigo de
  saida do sync. Grava em <Pasta>\logs\agendador.log uma linha de inicio (antes
  do sync) e uma de fim; inicio sem fim = processo morto (ex.: limite de 3 h).
  Binario ou pasta ausentes: erro claro e saida diferente de zero.

  Alertas (tarefa 3.6):
  - sync com codigo de saida 3 = trava ocupada (outra execucao em andamento):
    chama `sync alerta --geral "trava ocupada: ..."`, que nao pega a trava;
  - processo morto: a ultima linha do agendador.log e um `inicio` cujo PID
    (gravado na propria linha, `pid=`) nao existe mais. Antes de comecar, o
    script fecha essa linha no log e chama `sync alerta --geral "processo
    morto: ..."`. Linha de inicio sem `pid=` (formato antigo) nao e avaliada;
  - rascunho novo em <Pasta>\alertas\*.eml (comparando a lista antes e depois)
    vira uma linha AVISO no agendador.log e na tela;
  - binario ou pasta ausentes ficam so no agendador.log e no "Ultimo
    resultado" do Agendador (sem binario nao ha como gerar o alerta).

  Windows PowerShell 5.1. Arquivo so em ASCII de proposito (o 5.1 le UTF-8 sem
  BOM como ANSI e estragaria os acentos).

.PARAMETER Pasta
  Pasta de trabalho (fixa sync.lock, logs/, alertas/ e os bancos). Padrao: raiz
  do repositorio (pai de scripts/).

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
# Rascunhos que ja existiam antes do sync; $null = ainda nao capturados
# (saidas antecipadas nao comparam, para nao chamar de novo um rascunho antigo).
$script:antes = $null

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

# Nomes dos rascunhos de alerta (alertas\*.eml) que ainda nao foram para enviados\.
function Listar-Rascunhos([string]$pastaTrab) {
    try {
        $dir = Join-Path $pastaTrab 'alertas'
        if (-not (Test-Path -LiteralPath $dir -PathType Container)) { return @() }
        return @(Get-ChildItem -LiteralPath $dir -Filter '*.eml' -File | ForEach-Object { $_.Name })
    } catch {
        return @()
    }
}

function Finalizar([int]$codigo, [string]$detalhe) {
    # Pasta ausente: o log cai na raiz do repositorio, para ainda deixar rastro.
    $destino = $Pasta
    if (-not (Test-Path -LiteralPath $Pasta -PathType Container)) { $destino = $raizRepo }
    # Aviso de rascunho novo: no agendador.log (antes da linha de fim) e na tela.
    if ($null -ne $script:antes) {
        $depois = Listar-Rascunhos $Pasta
        foreach ($nome in $depois) {
            if ($script:antes -notcontains $nome) {
                $aviso = "AVISO: rascunho de alerta novo: alertas\$nome (abra, envie e mova para alertas\enviados\)"
                Gravar-Agendador $destino ('{0} {1}' -f (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'), $aviso)
                [Console]::Error.WriteLine($aviso)
            }
        }
    }
    $fim = Get-Date
    $linha = '{0} -> {1} | exit {2} | {3} | pasta={4} | binario={5} | args={6}' -f `
        $inicio.ToString('yyyy-MM-dd HH:mm:ss'), $fim.ToString('HH:mm:ss'), $codigo, $detalhe, $Pasta, $Binario, $Argumentos
    Gravar-Agendador $destino $linha
    exit $codigo
}

# `sync alerta --geral`: nao pega a trava, entao roda mesmo com outro sync vivo.
# Precisa da pasta de trabalho como diretorio atual (alertas\ e logs\ sao relativos).
# Falha daqui nunca muda o codigo de saida da execucao: so deixa rastro.
function Chamar-Alerta([string]$motivo) {
    try {
        $semAspas = $motivo.Replace('"', "'")
        & $Binario alerta --geral $semAspas
        $c = $LASTEXITCODE
        if ($c -ne 0) {
            Gravar-Agendador $Pasta ('{0} AVISO sync alerta --geral saiu com codigo {1}' -f (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'), $c)
        }
    } catch {
        Gravar-Agendador $Pasta ('{0} AVISO nao foi possivel chamar sync alerta --geral: {1}' -f (Get-Date).ToString('yyyy-MM-dd HH:mm:ss'), $_.Exception.Message)
    }
}

# Execucao anterior morta: a ultima linha de inicio/fim do agendador.log e um
# `inicio` com `pid=` de processo que nao existe mais (ou que nao e o PowerShell).
# Devolve a linha de inicio, ou $null.
function Achar-ProcessoMorto([string]$pastaTrab) {
    try {
        $arq = Join-Path (Join-Path $pastaTrab 'logs') 'agendador.log'
        if (-not (Test-Path -LiteralPath $arq -PathType Leaf)) { return $null }
        $ultima = $null
        foreach ($l in @(Get-Content -LiteralPath $arq -Encoding UTF8 -Tail 100)) {
            if ($l -match ' inicio \| ' -or $l -match ' -> ') { $ultima = $l }
        }
        if ($null -eq $ultima -or $ultima -notmatch ' inicio \| ') { return $null }
        if (-not ($ultima -match 'pid=(\d+)')) { return $null }
        $vivo = Get-Process -Id ([int]$Matches[1]) -ErrorAction SilentlyContinue
        if ($null -ne $vivo -and $vivo.ProcessName -match '^(powershell|pwsh)') {
            # PID reaproveitado: o processo vivo precisa ter nascido antes do
            # `inicio` registrado (2 s de folga). Se nao der para ler a hora de
            # inicio ou a hora da linha, na duvida vale "vivo" (nao acusa a toa).
            $nasceu = $null
            try { $nasceu = $vivo.StartTime } catch { $nasceu = $null }
            $iniciou = $null
            try {
                $iniciou = [datetime]::ParseExact($ultima.Substring(0, 19), 'yyyy-MM-dd HH:mm:ss', [System.Globalization.CultureInfo]::InvariantCulture)
            } catch { $iniciou = $null }
            if ($null -eq $nasceu -or $null -eq $iniciou -or $nasceu -le $iniciou.AddSeconds(2)) { return $null }
        }
        return $ultima
    } catch {
        return $null
    }
}

if (-not (Test-Path -LiteralPath $Pasta -PathType Container)) {
    [Console]::Error.WriteLine("ERRO: pasta de trabalho nao existe: $Pasta")
    Finalizar 2 'ERRO pasta ausente'
}
if (-not (Test-Path -LiteralPath $Binario -PathType Leaf)) {
    [Console]::Error.WriteLine("ERRO: binario nao encontrado: $Binario (rode cargo build --release)")
    Finalizar 2 'ERRO binario ausente'
}

try {
    Set-Location -LiteralPath $Pasta -ErrorAction Stop
} catch {
    [Console]::Error.WriteLine("ERRO: nao foi possivel entrar na pasta de trabalho: $($_.Exception.Message)")
    Finalizar 1 'ERRO ao entrar na pasta'
}

$script:antes = Listar-Rascunhos $Pasta

# Processo morto: fecha a linha pendente (para nao avisar de novo na proxima vez)
# e conta uma falha no alvo "geral".
$morta = Achar-ProcessoMorto $Pasta
if ($null -ne $morta) {
    $quando = ($morta -split ' inicio ')[0]
    $agora = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
    [Console]::Error.WriteLine("AVISO: a execucao anterior ($quando) nao terminou: processo morto")
    Gravar-Agendador $Pasta ('{0} -> {1} | exit - | AVISO processo morto: a execucao iniciada em {0} nao terminou | pasta={2}' -f $quando, $agora, $Pasta)
    Chamar-Alerta ('processo morto: a execucao iniciada em {0} nao terminou (sem linha de fim no agendador.log)' -f $quando)
}

# Linha de inicio: se o Agendador matar o processo (limite de 3 h), sobra rastro.
# O PID deixa a proxima execucao distinguir "morto" de "ainda rodando".
Gravar-Agendador $Pasta ('{0} inicio | pasta={1} | binario={2} | args={3} | pid={4}' -f `
    $inicio.ToString('yyyy-MM-dd HH:mm:ss'), $Pasta, $Binario, $Argumentos, $PID)

try {
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

if ($codigo -eq 0) { Finalizar 0 'sucesso' }

# Codigo 3 = trava ocupada (outra execucao em andamento): nada rodou, entao o
# proprio sync nao registrou alerta; conta uma falha no alvo "geral".
if ($codigo -eq 3) {
    $dono = ''
    try {
        $arqDono = Join-Path $Pasta 'sync.lock.dono'
        if (Test-Path -LiteralPath $arqDono -PathType Leaf) {
            $dono = ((@(Get-Content -LiteralPath $arqDono -TotalCount 5) -join ' / ').Trim())
            if ($dono.Length -gt 200) { $dono = $dono.Substring(0, 200) }
        }
    } catch { $dono = '' }
    $motivo = 'trava ocupada: o sync saiu com codigo 3 (outra execucao em andamento)'
    if ($dono) { $motivo = $motivo + '; dono: ' + $dono }
    Chamar-Alerta $motivo
    Finalizar $codigo 'trava ocupada (exit 3)'
}

Finalizar $codigo 'falha do sync'
