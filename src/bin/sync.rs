use clap::{Parser, Subcommand, ValueEnum};
use rust_spider::avaliacao::{self, AvaliacaoFaixa, Cauda, Comparacao, Intervalo};
use rust_spider::log_arquivo::{self, LogArquivo, Nivel};
use rust_spider::orquestracao::Destino;
use rust_spider::orquestracao::{
    carregar_existentes, descrever_faixa, faixas_faltando, hoje_de, validar_lote, PastasAtualizar,
};
use rust_spider::{
    backup, config, dataset, db, downloader, exportacao, manuais, orquestracao, perfil,
    posicoes as posicoes_relatorio, publicacao, ranking, registros, tabela, tempo, trava,
    validacao, SpiderError,
};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

/// Log em arquivo da execução (3.5), aberto em `main` só para os comandos que
/// gravam. Todo texto que vai para a tela passa por [`emitir`], que o copia
/// aqui; sem log aberto (comandos de leitura, ou log que falhou) só imprime.
static LOG: Mutex<Option<LogArquivo>> = Mutex::new(None);

/// Mostra `texto` na saída padrão ou de erro e o copia no log com o `nivel`.
/// É o único caminho de saída do binário. O log vem antes da tela: se a saída
/// estiver fechada (`| head`) e o `println!` entrar em pânico, o registro já
/// foi feito.
fn emitir(nivel: Nivel, destino: Destino, texto: &str) {
    registrar_no_log(nivel, texto);
    match destino {
        Destino::Padrao => std::println!("{texto}"),
        Destino::Erro => std::eprintln!("{texto}"),
    }
}

/// Aviso na tela (fora do log, que já parou) de que o log foi desligado.
fn avisar_log_desligado(erro: SpiderError) {
    std::eprintln!(
        "Aviso: o log em arquivo parou de gravar e foi desligado; o comando segue sem log: {:#}",
        anyhow::Error::from(erro)
    );
}

/// Copia no log, se aberto. Falha de escrita: avisa na tela uma vez (o log
/// se desliga sozinho) e o comando segue.
fn registrar_no_log(nivel: Nivel, texto: &str) {
    let mut guarda = LOG.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(log) = guarda.as_mut() {
        if let Err(e) = log.registrar(nivel, texto) {
            avisar_log_desligado(e);
        }
    }
}

// Estas duas macros trocam o `println!`/`eprintln!` da biblioteca padrão em
// todo este arquivo: o texto na tela é o mesmo, mas também vai para o log.
// `println!` é nível INFO; `eprintln!` é ERRO. Mensagem de outro nível usa
// `emitir` direto.
macro_rules! println {
    () => {
        emitir(Nivel::Info, Destino::Padrao, "")
    };
    ($($arg:tt)*) => {
        emitir(Nivel::Info, Destino::Padrao, &format!($($arg)*))
    };
}

macro_rules! eprintln {
    () => {
        emitir(Nivel::Erro, Destino::Erro, "")
    };
    ($($arg:tt)*) => {
        emitir(Nivel::Erro, Destino::Erro, &format!($($arg)*))
    };
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Baixa e processa resultados de loterias da Caixa (planilha .xlsx -> SQLite/JSON)"
)]
struct Cli {
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Navega até o site oficial, baixa a planilha (.xlsx) de resultados e já grava no banco.
    Baixar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta onde salvar a planilha (.xlsx) baixada
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Caminho do banco SQLite. Por padrão, um arquivo por loteria
        /// (resultados_<loteria>.db), para não misturar dados de loterias diferentes.
        /// Não pode ser usado com `todas`.
        #[arg(long)]
        db: Option<PathBuf>,
        /// Sobrescreve o seletor CSS do botão de download configurado para essa
        /// loteria. Não pode ser usado com `todas`.
        #[arg(long)]
        seletor: Option<String>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Processa uma planilha (.xlsx) de resultados já baixada manualmente, sem abrir navegador.
    Importar {
        /// Slug da loteria (define como interpretar as colunas da tabela);
        /// não aceita `todas`, pois o arquivo é de uma loteria só
        loteria: String,
        /// Caminho da planilha (.xlsx) de resultados
        arquivo: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Lista os resultados já salvos no banco, em JSON.
    Listar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lê o último resultado direto da página (sem baixar a planilha inteira)
    /// e grava no banco só se for um concurso novo. Se faltar concurso entre o
    /// último salvo e o da página (salto), copia o banco para `backups/`
    /// (nome com data e hora em UTC) e baixa a planilha completa, como `baixar`.
    Atualizar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta onde salvar a planilha (.xlsx), se houver salto
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Pasta dos backups do banco feitos antes de baixar a planilha
        /// (resultados_<loteria>_<AAAAMMDD-HHMMSS>.db, horário em UTC)
        #[arg(long, default_value = backup::PASTA_PADRAO)]
        backups: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Confere a integridade dos resultados salvos: quantidade de dezenas,
    /// faixa, repetidas, datas, buracos na numeração e datas fora de ordem.
    /// Sai com erro se encontrar algum erro; datas fora de ordem são só avisos.
    Validar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Lista os concursos que faltam no banco, com as datas dos vizinhos,
    /// para fechá-los um a um com `inserir`.
    Buracos {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Insere à mão um concurso que falta no banco (fecha um buraco) e o
    /// registra no cache de inserções manuais, em ordem de data de sorteio.
    Inserir {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Número do concurso que falta
        concurso: u32,
        /// Data do sorteio, no formato AAAA-MM-DD
        #[arg(long)]
        data: String,
        /// Dezenas sorteadas, separadas por vírgula (ex.: 1,5,10,22,40)
        #[arg(long)]
        dezenas: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
    },
    /// Calcula as features de cada dezena e janela (série temporal) e grava em
    /// `features_dezena`, retomando do último concurso já calculado. Recusa
    /// rodar se o `validar` apontar erro (buraco incluso).
    Features {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Ignora o estado salvo e recalcula todas as features do zero
        #[arg(long)]
        completo: bool,
    },
    /// Calcula o índice de força (total, posição e variação de posição) de
    /// cada dezena em cada concurso com um perfil de pesos e grava em `indice`,
    /// regenerando o índice inteiro do perfil. Recusa rodar se as features não
    /// estiverem em dia (rode `features` antes).
    Indice {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Nome do perfil de pesos (arquivo `<perfis>/<loteria>/<nome>.toml`,
        /// ex.: padrao_v1)
        #[arg(long)]
        perfil: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
    },
    /// Avalia o índice de força de um perfil contra o acaso: log loss, Brier
    /// e top-k em 5 dobras de walk-forward no trecho de busca, e o veredito
    /// "supera o acaso" (bootstrap por blocos) no trecho inteiro. Só lê o
    /// banco; recusa se o índice ou as features não estiverem em dia.
    Avaliar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Nome do perfil de pesos (já registrado pelo `indice`)
        #[arg(long)]
        perfil: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
        /// k do top-k (padrão: K, as dezenas sorteadas por concurso); só na
        /// busca: o trancado usa sempre K
        #[arg(long, conflicts_with = "trancado")]
        k: Option<u32>,
        /// Avalia o conjunto TRANCADO (concursos acima do corte da busca), de
        /// uso único por perfil: o uso fica registrado em
        /// `registros/trancado_<loteria>.json`, na pasta do banco (versionado
        /// no git)
        #[arg(long)]
        trancado: bool,
        /// Com `--trancado`: avalia de novo um perfil que já usou o trancado
        /// (mostra as avaliações anteriores; a nova também é registrada)
        #[arg(long, requires = "trancado")]
        repetir: bool,
        /// Quantos perfis/k foram testados na busca: mostra também os
        /// p-valores com Bonferroni (p·N, limitado a 1)
        #[arg(long, conflicts_with = "trancado")]
        testados: Option<u32>,
        /// Só para testes (escondida na ajuda): pasta dos registros de uso do
        /// trancado. O padrão é `registros/` ao lado do banco, e trocar a
        /// pasta tira a proteção do uso único e o Bonferroni do registro
        /// versionado.
        #[arg(long, hide = true)]
        registros: Option<PathBuf>,
    },
    /// Grava em CSV a tabela geral de um concurso (todas as dezenas em ordem
    /// de posição, com total, variação, `saiu` e as features da janela do
    /// perfil), para abrir no Excel e comparar ao longo do tempo: um arquivo
    /// por concurso em `<saida>/<loteria>/`. Padrão: o próximo concurso,
    /// ainda não sorteado. Só lê o banco; recusa se o índice ou as features
    /// não estiverem em dia.
    Tabela {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Nome do perfil de pesos (já registrado pelo `indice`)
        #[arg(long)]
        perfil: String,
        /// Concurso da tabela (padrão: o próximo, ainda não sorteado); um
        /// concurso já sorteado sai com a coluna `saiu` preenchida. Não pode
        /// ser usado com `todas`
        #[arg(long)]
        concurso: Option<u32>,
        /// Pasta onde gravar os CSV (uma subpasta por loteria)
        #[arg(long, default_value = "tabelas")]
        saida: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
    },
    /// Ranking por taxa histórica de posição e componentes (Fase 1f): cruza a
    /// posição que cada dezena tinha na tabela de força (e a faixa de cada
    /// componente) antes de cada sorteio com o resultado, e ordena as dezenas
    /// do próximo concurso pela taxa de saída da posição (nota A, principal) e
    /// por um naive Bayes sobre a posição e as faixas (nota B, EXPERIMENTAL).
    /// Grava `ranking_taxa`, `taxa_posicao` e `taxa_faixa` no banco
    /// (regenerando as do perfil), imprime o relatório e grava o CSV do
    /// próximo concurso em `<saida>/<loteria>/`. É descrição do passado, não
    /// previsão: os sorteios são independentes. Recusa se o índice ou as
    /// features não estiverem em dia. Com `--avaliar`, em vez de gravar,
    /// avalia o ranking (A e B) contra o acaso nas dobras de teste 2 a 5 da
    /// busca, só lendo o banco.
    Ranking {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Nome do perfil de pesos, já registrado pelo `indice` (padrão: o
        /// perfil ativo da loteria)
        #[arg(long)]
        perfil: Option<String>,
        /// Pasta onde gravar o CSV (uma subpasta por loteria)
        #[arg(long, default_value = "ranking")]
        saida: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
        /// Em vez de gravar, avalia o ranking contra o acaso: A (top-k pela
        /// ordem de A e log loss com a calibração linear) e B (experimental:
        /// log loss, Brier, top-k, bootstrap e a linha linear), só nas dobras
        /// de teste 2 a 5 do walk-forward da busca. Só lê o banco: não grava
        /// tabelas nem CSV. O conjunto trancado fica fora (não há
        /// `--trancado`). Descrição do passado, não previsão
        #[arg(long, conflicts_with = "saida")]
        avaliar: bool,
    },
    /// Índice de força das posições (Fase 1g): marca, em cada concurso, as
    /// posições que a tabela de força ocupava nas dezenas sorteadas, trata
    /// cada posição como se fosse uma dezena e calcula sobre elas as features
    /// e o índice de força (G-1). Grava só nas tabelas `posicao_marcas`,
    /// `posicao_features`, `posicao_indice`, `posicao_perfil` e
    /// `posicao_motor_estado`; não altera nenhuma tabela existente, nem o
    /// `perfis`. Retoma do estado salvo e calcula só os concursos novos.
    /// Imprime o relatório e grava o CSV do próximo concurso em
    /// `<saida>/<loteria>/`. É descrição do passado, não previsão: os sorteios
    /// são independentes. Recusa se o índice de força ou as features não
    /// estiverem em dia.
    Posicoes {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Perfil de pesos que dá as posições, já registrado pelo `indice`
        /// (padrão: o perfil ativo da loteria)
        #[arg(long)]
        perfil: Option<String>,
        /// Perfil de pesos do índice das posições (arquivo
        /// `<perfis>/<loteria>/<nome>.toml`); fica registrado em `posicao_perfil`
        #[arg(long, default_value = "posicoes_v1")]
        perfil_posicoes: String,
        /// Pasta onde gravar o CSV (uma subpasta por loteria)
        #[arg(long, default_value = "posicoes")]
        saida: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
        /// Em vez de gravar, avalia a ordem G-1 (as dezenas ordenadas pelo
        /// índice das posições) contra o acaso, com a nota A da 1f como
        /// referência no mesmo recorte: log loss, Brier, top-k, bootstrap e a
        /// linha linear, só nas dobras de teste 2 a 5 do walk-forward da
        /// busca, sem os 100 primeiros concursos (aquecimento). Só lê o banco:
        /// não grava tabelas nem CSV. O conjunto trancado fica fora (não há
        /// `--trancado`). Descrição do passado, não previsão
        #[arg(long, conflicts_with = "saida")]
        avaliar: bool,
    },
    /// Exporta os JSON do contrato `v1` para o app: `<saida>/v1/index.json` e,
    /// por loteria, `ultimo.json`, `tabela_geral.json` (snapshot do próximo
    /// concurso com o perfil ativo) e `historico.json`, com `schema_version` e
    /// SHA-256 de cada arquivo no índice. Só lê o banco; recusa, sem gravar
    /// nada, se as features ou o índice do perfil não estiverem em dia. Com
    /// `todas`, é tudo ou nada; com um slug, regrava só os arquivos dele e
    /// refaz o índice com o que está no disco.
    Exportar {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta de saída (os arquivos vão em `<saida>/v1/`)
        #[arg(long, default_value = "publico")]
        saida: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
    },
    /// Publica o `<saida>/v1/` já exportado no clone git do repositório de
    /// dados e envia (push para `origin/main`). Não chama o `exportar` nem lê
    /// banco: confere `sha256` e `bytes` do `index.json` contra o disco e
    /// recusa se não baterem. Antes de copiar faz `git fetch` e recusa, sem
    /// tocar em nada, se o clone estiver sujo, fora de `main` ou atrás/
    /// divergente de `origin/main`. Se só `gerado_em` mudou, diz "nada a
    /// publicar" e sai com sucesso. Um commit por publicação; se o push
    /// falhar, o commit fica local e a próxima execução tenta de novo. Não
    /// clona sozinho.
    Publicar {
        /// Pasta de saída do `exportar` (lê `<saida>/v1/`)
        #[arg(long, default_value = "publico")]
        saida: PathBuf,
        /// Clone do repositório de dados
        #[arg(long, default_value = publicacao::REPO_PADRAO)]
        repo: PathBuf,
    },
    /// Calendário de sorteios (só leitura): diz se hoje (Brasília) é dia de
    /// sorteio e desde que horário procurar o resultado, em que estado está o
    /// último concurso do banco (em_dia, aguardando ou atrasado) e avisa se o
    /// calendário configurado diverge dos dias dos últimos concursos. Avisos e
    /// estados não são erro: o comando só falha se não conseguir ler o banco.
    Calendario {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Consulta outro dia (AAAA-MM-DD) em vez de hoje: muda só a parte
        /// "é dia de sorteio / procurar desde"; o estado do banco segue o
        /// relógio de agora
        #[arg(long)]
        data: Option<String>,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Ciclo completo por loteria: atualizar -> backfill (se houver buraco) ->
    /// validar -> features (incremental) -> índice do perfil ativo -> exportar;
    /// depois de todas, um único publicar. Uma loteria com falha não
    /// interrompe as outras: só as que passaram são exportadas, o publicar
    /// roda mesmo assim e o comando sai com erro no fim. Sem concurso novo,
    /// segue até o publicar (passos idempotentes). Backfill = baixar a
    /// planilha inteira; se o buraco persistir, só aviso e o validar reprova.
    /// Com `--aguardar`, repete em rodadas (5, 10, 20, 30, 30... min) até
    /// 01h00 de Brasília, só para quem falta, e publica cada loteria que chega.
    Ciclo {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Pasta onde salvar a planilha (.xlsx), se houver salto ou backfill
        #[arg(long, default_value = "downloads")]
        dir: PathBuf,
        /// Pasta dos backups do banco feitos antes de baixar a planilha
        #[arg(long, default_value = backup::PASTA_PADRAO)]
        backups: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
        /// Mostra a janela do navegador (útil para depurar bloqueios/seletores)
        #[arg(long)]
        visivel: bool,
        /// Pasta do cache de concursos inseridos à mão (manuais_<loteria>.json)
        #[arg(long, default_value = "cache")]
        cache: PathBuf,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
        /// Pasta de saída do exportar (os arquivos vão em `<saida>/v1/`)
        #[arg(long, default_value = "publico")]
        saida: PathBuf,
        /// Clone do repositório de dados
        #[arg(long, default_value = publicacao::REPO_PADRAO)]
        repo: PathBuf,
        /// Repete o ciclo enquanto o concurso esperado não sai (dia de sorteio
        /// e a página ainda mostra o anterior) ou o navegador/rede falha de
        /// passagem; erro de validação ou de gravação não é repetido. A janela
        /// vai da primeira passada até 01h00 de Brasília (pensado para o
        /// disparo das 23h00); cada loteria que chega é publicada na hora.
        /// Fora de dia de sorteio ou com a loteria em dia, não espera.
        #[arg(long)]
        aguardar: bool,
    },
    /// Grava o dataset de ML para o Python: as features de `features_dezena`
    /// (uma linha por concurso, dezena e janela, com data, trecho e `saiu`)
    /// dos concursos já sorteados, em `<saida>/<loteria>/features_v<versão>.<ext>`,
    /// com `manifesto_<ext>.json` ao lado (corte, linhas, SHA-256, impressões
    /// dos resultados do banco e dos incluídos). Por padrão só o trecho de
    /// busca (até o corte); o trancado fica fora. Com `--trancado`, grava em
    /// `features_v<versão>_trancado.<ext>` + `manifesto_trancado_<ext>.json`,
    /// sem tocar nos arquivos da busca. Só lê o banco; recusa se as features
    /// não estiverem em dia.
    Dataset {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); não aceita `todas`
        loteria: String,
        /// Formato do arquivo de dados
        #[arg(long, value_enum, default_value_t = FormatoDataset::Parquet)]
        formato: FormatoDataset,
        /// Inclui o conjunto TRANCADO (concursos acima do corte da busca).
        /// Só para a validação final do perfil campeão: nada acima do corte
        /// pode entrar na busca de pesos
        #[arg(long)]
        trancado: bool,
        /// Pasta onde gravar (uma subpasta por loteria)
        #[arg(long, default_value = "datasets")]
        saida: PathBuf,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Testes de aleatoriedade do histórico inteiro (qui-quadrado de
    /// frequência e runs test por dezena, com Holm). Só lê o banco.
    Aleatoriedade {
        /// Slug da loteria (ex.: lotofacil, megasena, quina) ou `todas`
        loteria: String,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db);
        /// não pode ser usado com `todas`
        #[arg(long)]
        db: Option<PathBuf>,
    },
    /// Evidência bayesiana (fator de Bayes em cinco graus): (A) por dezena, para
    /// o próximo concurso, com o histórico inteiro e os últimos 500 concursos;
    /// (B) do índice de força de um perfil, nas dobras de teste 2 a 5 do
    /// walk-forward. Só lê o banco: não grava nada.
    Evidencia {
        /// Slug da loteria (ex.: lotofacil, megasena, quina); `todas` não é aceito
        loteria: String,
        /// Perfil de pesos de B (arquivo `<perfis>/<loteria>/<nome>.toml`, já
        /// registrado pelo `indice`). Padrão: o perfil ativo da loteria
        /// (`padrao_v1`); se B não puder ser calculada com um perfil dado aqui,
        /// o comando termina com erro (com o perfil padrão, só avisa)
        #[arg(long)]
        perfil: Option<String>,
        /// Caminho do banco SQLite (padrão: resultados_<loteria>.db)
        #[arg(long)]
        db: Option<PathBuf>,
        /// Pasta dos perfis de pesos (uma subpasta por loteria)
        #[arg(long, default_value = "perfis")]
        perfis: PathBuf,
    },
    /// Registra uma falha do "geral" (a execução em si, não uma loteria) no
    /// histórico de alertas e grava o rascunho de e-mail quando a regra manda
    /// (2 noites seguidas com o mesmo motivo). É o que o
    /// `scripts/ciclo_agendado.ps1` chama quando o `sync` sai com código 3
    /// (trava ocupada) e quando acha uma execução anterior sem fechamento
    /// (processo morto). Não pega a trava `sync.lock`: roda mesmo com outro
    /// `sync` em andamento.
    Alerta {
        /// Motivo da falha geral, texto livre. A parte antes do primeiro `:`
        /// identifica o motivo na repetição (ex.: "trava ocupada: PID 4120").
        #[arg(long)]
        geral: String,
    },
    /// Lista os rascunhos de e-mail de alerta (`alertas/*.eml`) que ainda não
    /// foram movidos para `alertas/enviados/`. Só lê; não pega a trava.
    Alertas,
}

/// Formato do `sync dataset` na linha de comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FormatoDataset {
    Parquet,
    Csv,
}

impl From<FormatoDataset> for dataset::Formato {
    fn from(f: FormatoDataset) -> Self {
        match f {
            FormatoDataset::Parquet => dataset::Formato::Parquet,
            FormatoDataset::Csv => dataset::Formato::Csv,
        }
    }
}

/// Por que `dataset` não aceita `todas`.
const MOTIVO_DATASET_UMA_LOTERIA: &str =
    "gere o dataset de uma loteria por vez (cada uma tem seu banco, suas features e seu corte)";

/// Por que `evidencia` não aceita `todas`.
const MOTIVO_EVIDENCIA_UMA_LOTERIA: &str =
    "calcule uma loteria por vez (cada uma tem seu banco e suas dezenas)";

/// Por que `indice` não aceita `todas`.
const MOTIVO_INDICE_UMA_LOTERIA: &str =
    "calcule uma loteria por vez (cada uma tem seu banco e seus perfis)";

/// Por que `avaliar` não aceita `todas`.
const MOTIVO_AVALIAR_UMA_LOTERIA: &str =
    "avalie uma loteria e um perfil por vez (cada loteria tem seu banco, seus perfis e seu trancado)";

/// Por que `ranking` não aceita `todas`.
const MOTIVO_RANKING_UMA_LOTERIA: &str =
    "calcule uma loteria por vez (cada uma tem seu banco, seus perfis e suas tabelas)";

/// Por que `posicoes` não aceita `todas`.
const MOTIVO_POSICOES_UMA_LOTERIA: &str =
    "calcule uma loteria por vez (cada uma tem seu banco, seus perfis e suas tabelas)";

/// Nome do comando se ele grava (e portanto toma a trava `sync.lock`, 3.4);
/// `None` para os que só leem. Sem `_`: comando novo obriga a decidir aqui.
/// `tabela` fica livre: lê o banco e grava só em `tabelas/`, que nenhum outro
/// comando usa. `avaliar` só grava (o registro do trancado) com `--trancado`.
fn nome_se_grava(comando: &Comando) -> Option<&'static str> {
    match comando {
        Comando::Baixar { .. } => Some("baixar"),
        Comando::Importar { .. } => Some("importar"),
        Comando::Atualizar { .. } => Some("atualizar"),
        Comando::Inserir { .. } => Some("inserir"),
        Comando::Features { .. } => Some("features"),
        Comando::Indice { .. } => Some("indice"),
        Comando::Exportar { .. } => Some("exportar"),
        Comando::Publicar { .. } => Some("publicar"),
        Comando::Ciclo { .. } => Some("ciclo"),
        Comando::Dataset { .. } => Some("dataset"),
        Comando::Ranking { .. } => Some("ranking"),
        Comando::Posicoes { .. } => Some("posicoes"),
        Comando::Avaliar { trancado, .. } => trancado.then_some("avaliar --trancado"),
        Comando::Listar { .. }
        | Comando::Validar { .. }
        | Comando::Buracos { .. }
        | Comando::Tabela { .. }
        | Comando::Calendario { .. }
        | Comando::Aleatoriedade { .. }
        | Comando::Evidencia { .. }
        | Comando::Alerta { .. }
        | Comando::Alertas => None,
    }
}

/// Código de saída da trava ocupada (3.6): o `ciclo_agendado.ps1` o usa para
/// chamar `sync alerta --geral`. Qualquer outra falha sai com 1.
const CODIGO_TRAVA_OCUPADA: u8 = 3;

/// O comando deixa rastro no log do dia: os que gravam (mesma regra da
/// trava) e o `alerta`, que grava só em `alertas/` e por isso não toma a
/// trava, mas precisa aparecer no log da noite.
fn grava_log(comando: &Comando) -> bool {
    nome_se_grava(comando).is_some() || matches!(comando, Comando::Alerta { .. })
}

/// Código de saída de uma execução que falhou: 3 só para a trava ocupada.
fn codigo_de_falha(erro: &anyhow::Error) -> std::process::ExitCode {
    let trava_ocupada = erro
        .downcast_ref::<SpiderError>()
        .is_some_and(|e| matches!(e, SpiderError::TravaOcupada { .. }));
    if trava_ocupada {
        std::process::ExitCode::from(CODIGO_TRAVA_OCUPADA)
    } else {
        std::process::ExitCode::FAILURE
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    // O log abre antes da trava: um `ciclo` agendado que encontre a trava
    // ocupada precisa deixar rastro no arquivo do dia.
    abrir_log(&cli.comando);
    let resultado = executar(cli).await;
    // Mesma saída que o `main` devolvendo `anyhow::Result` produzia
    // (`Error: {:?}`, com a cadeia "Caused by"), agora também no log.
    if let Err(erro) = &resultado {
        eprintln!("Error: {erro:?}");
    }
    fechar_log(resultado.is_ok());
    match &resultado {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(erro) => codigo_de_falha(erro),
    }
}

/// Abre o log do dia e escreve a linha de abertura, se o comando grava
/// (mesma regra da trava, [`nome_se_grava`]). Qualquer falha vira um aviso na
/// tela e o comando segue sem log.
fn abrir_log(comando: &Comando) {
    if !grava_log(comando) {
        return;
    }
    let aberto = LogArquivo::abrir(
        std::path::Path::new(log_arquivo::PASTA),
        Box::new(agora_segundos),
    );
    let mut log = match aberto {
        Ok(log) => log,
        Err(e) => {
            std::eprintln!(
                "Aviso: não foi possível abrir o log em arquivo; o comando segue sem log: {:#}",
                anyhow::Error::from(e)
            );
            return;
        }
    };
    // argv[0] é o caminho do executável; no log basta o nome (`sync`).
    let mut argv: Vec<String> = std::env::args().collect();
    if let Some(primeiro) = argv.first_mut() {
        if let Some(nome) = std::path::Path::new(primeiro.as_str()).file_stem() {
            *primeiro = nome.to_string_lossy().into_owned();
        }
    }
    if let Err(e) = log.abertura(std::process::id(), &argv, env!("CARGO_PKG_VERSION")) {
        avisar_log_desligado(e);
    }
    *LOG.lock().unwrap_or_else(PoisonError::into_inner) = Some(log);
    // Diagnósticos de dentro da biblioteca (tentativas do navegador, layout da
    // planilha) seguem o mesmo caminho da tela e do log.
    log_arquivo::instalar_diagnostico(|nivel, texto| emitir(nivel, Destino::Erro, texto));
}

/// Linha de fechamento (duração, sucesso ou falha) e fecha o log.
fn fechar_log(sucesso: bool) {
    let mut guarda = LOG.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(mut log) = guarda.take() {
        if let Err(e) = log.fechamento(sucesso) {
            avisar_log_desligado(e);
        }
    }
}

async fn executar(cli: Cli) -> anyhow::Result<()> {
    // A trava vem antes de qualquer gravação e fica até o fim da execução
    // (`ciclo --aguardar`: a noite toda). Trava ocupada: erro na hora.
    let _trava = match nome_se_grava(&cli.comando) {
        Some(nome) => Some(trava::Trava::obter(
            std::path::Path::new("."),
            nome,
            i64::try_from(agora_segundos()?).unwrap_or(i64::MAX),
        )?),
        None => None,
    };

    match cli.comando {
        Comando::Baixar {
            loteria,
            dir,
            db,
            seletor,
            visivel,
            cache,
        } => {
            let alvos = resolver_alvos(&loteria, db, seletor.as_deref())?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = baixar(cfg, &dir, &db_caminho, seletor.as_deref(), visivel, &cache).await;
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Importar {
            loteria,
            arquivo,
            db,
            cache,
        } => {
            let cfg = buscar_config_unica(&loteria, "importar", "o arquivo é de uma loteria só")?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let hoje = hoje_de(agora_segundos)?;
            orquestracao::processar_e_salvar(
                &arquivo,
                cfg,
                &db_caminho,
                &cache,
                hoje,
                &mut imprimir_evento,
            )?;
        }
        Comando::Listar { loteria, db } => {
            let cfg = buscar_config_unica(
                &loteria,
                "listar",
                "liste uma loteria por vez (cada uma tem seu banco)",
            )?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let resultados = listar(cfg, &db_caminho)?;
            println!("{}", serde_json::to_string_pretty(&resultados)?);
        }
        Comando::Atualizar {
            loteria,
            dir,
            backups,
            db,
            visivel,
            cache,
        } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            let pastas = PastasAtualizar {
                backups: &backups,
                cache: &cache,
            };
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = atualizar(cfg, &db_caminho, &dir, &pastas, visivel).await;
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Validar { loteria, db } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                execucao.registrar(cfg, validar_banco(cfg, &db_caminho))?;
            }
            execucao.concluir()?;
        }
        Comando::Buracos { loteria, db } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                execucao.registrar(cfg, listar_buracos(cfg, &db_caminho))?;
            }
            execucao.concluir()?;
        }
        Comando::Inserir {
            loteria,
            concurso,
            data,
            dezenas,
            db,
            cache,
        } => {
            let cfg = buscar_config_unica(&loteria, "inserir", "as dezenas são de uma loteria só")?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let entrada = manuais::EntradaManual {
                concurso,
                data_sorteio: data,
                dezenas: manuais::parsear_dezenas(&dezenas)?,
            };
            inserir_manual(cfg, entrada, &db_caminho, &cache)?;
        }
        Comando::Features {
            loteria,
            db,
            completo,
        } => {
            let cfg = buscar_config_unica(
                &loteria,
                "features",
                "calcule uma loteria por vez (cada uma tem seu banco)",
            )?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            calcular_features(cfg, &db_caminho, completo)?;
        }
        Comando::Indice {
            loteria,
            perfil,
            db,
            perfis,
        } => {
            let cfg = buscar_config_unica(&loteria, "indice", MOTIVO_INDICE_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            calcular_indice(cfg, &db_caminho, &perfis, &perfil)?;
        }
        Comando::Avaliar {
            loteria,
            perfil,
            db,
            perfis,
            k,
            trancado,
            repetir,
            testados,
            registros,
        } => {
            let cfg = buscar_config_unica(&loteria, "avaliar", MOTIVO_AVALIAR_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let opcoes = orquestracao::OpcoesAvaliacao {
                k,
                trancado,
                repetir,
                testados,
            };
            // O registro fica ao lado do banco, não no diretório de trabalho:
            // rodar de outra pasta com `--db` acha o mesmo registro.
            let padrao = registros::pasta_padrao(&db_caminho);
            let pasta_registros = PastaRegistros {
                escolhida: registros.unwrap_or_else(|| padrao.clone()),
                padrao,
            };
            avaliar(
                cfg,
                &db_caminho,
                &perfis,
                &perfil,
                &opcoes,
                &pasta_registros,
            )?;
        }
        Comando::Tabela {
            loteria,
            perfil,
            concurso,
            saida,
            db,
            perfis,
        } => {
            if concurso.is_some() && config::eh_todas(&loteria) {
                anyhow::bail!(
                    "`--concurso` não pode ser usado com `{}`: a numeração dos concursos é de \
                     cada loteria; informe o slug",
                    config::TODAS
                );
            }
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = gravar_tabela(cfg, &db_caminho, &perfis, &perfil, concurso, &saida);
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Ranking {
            loteria,
            perfil,
            saida,
            db,
            perfis,
            avaliar,
        } => {
            let cfg = buscar_config_unica(&loteria, "ranking", MOTIVO_RANKING_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let nome_perfil = perfil.as_deref().unwrap_or(cfg.perfil_ativo);
            if avaliar {
                avaliar_ranking(cfg, &db_caminho, &perfis, nome_perfil)?;
            } else {
                gravar_ranking(cfg, &db_caminho, &perfis, nome_perfil, &saida)?;
            }
        }
        Comando::Posicoes {
            loteria,
            perfil,
            perfil_posicoes,
            saida,
            db,
            perfis,
            avaliar,
        } => {
            let cfg = buscar_config_unica(&loteria, "posicoes", MOTIVO_POSICOES_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            let nome_perfil = perfil.as_deref().unwrap_or(cfg.perfil_ativo);
            if avaliar {
                avaliar_posicoes(cfg, &db_caminho, &perfis, nome_perfil, &perfil_posicoes)?;
            } else {
                gravar_posicoes(
                    cfg,
                    &db_caminho,
                    &perfis,
                    nome_perfil,
                    &perfil_posicoes,
                    &saida,
                )?;
            }
        }
        Comando::Exportar {
            loteria,
            saida,
            db,
            perfis,
        } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            exportar(&alvos, &perfis, &saida)?;
        }
        Comando::Publicar { saida, repo } => {
            let mut destino = publicacao::DestinoGit::novo(&repo);
            match publicacao::publicar(&mut destino, &saida)? {
                publicacao::Desfecho::Publicado { mensagem } => println!(
                    "Publicado e enviado para {}/{} a partir de {}: {mensagem}",
                    publicacao::REMOTO,
                    publicacao::BRANCH,
                    absoluto(&repo).display()
                ),
                publicacao::Desfecho::EnvioRetomado => println!(
                    "Nada novo a publicar, mas havia commit local sem push: enviado para {}/{}.",
                    publicacao::REMOTO,
                    publicacao::BRANCH
                ),
                publicacao::Desfecho::NadaAPublicar => println!(
                    "Nada a publicar: os arquivos são os mesmos do clone (só `gerado_em` mudou, se mudou)."
                ),
            }
        }
        Comando::Calendario { loteria, data, db } => {
            let dia = data
                .as_deref()
                .map(|texto| {
                    tempo::dias_da_data_iso(texto).ok_or_else(|| {
                        anyhow::anyhow!(
                            "`--data {texto}` inválida: use AAAA-MM-DD (ex.: 2026-10-13)"
                        )
                    })
                })
                .transpose()?;
            let alvos = resolver_alvos(&loteria, db, None)?;
            let agora = i64::try_from(agora_segundos()?).unwrap_or(i64::MAX);
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                let r = orquestracao::consultar_calendario(cfg, &db_caminho, agora, dia)
                    .map(|relatorio| imprimir_calendario(cfg, &relatorio, agora))
                    .map_err(anyhow::Error::from);
                execucao.registrar(cfg, r)?;
            }
            execucao.concluir()?;
        }
        Comando::Ciclo {
            loteria,
            dir,
            backups,
            db,
            visivel,
            cache,
            perfis,
            saida,
            repo,
            aguardar,
        } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            // A noite dos alertas é a do início da execução (o `--aguardar`
            // atravessa a meia-noite).
            let inicio = i64::try_from(agora_segundos()?).unwrap_or(i64::MAX);
            let pastas = orquestracao::PastasCiclo {
                backups: &backups,
                cache: &cache,
                perfis: &perfis,
                saida: &saida,
            };
            let mut destino = publicacao::DestinoGit::novo(&repo);
            if aguardar {
                let relatorio = orquestracao::executar_ciclo_aguardando(
                    &alvos.itens,
                    &pastas,
                    &mut destino,
                    &mut imprimir_evento,
                    &mut agora_segundos,
                    // Espera com `tokio::time::sleep`. A injeção em
                    // `executar_ciclo_aguardando` é síncrona (os testes
                    // trocam o sono por um relógio falso), então a ponte é
                    // `block_in_place`: o runtime (`#[tokio::main]`, várias
                    // threads) move as outras tarefas para outra thread
                    // enquanto esta espera, sem bloquear o executor.
                    &mut |espera| {
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(tokio::time::sleep(espera));
                        });
                    },
                    |cfg| downloader::raspar_ultimo_resultado(cfg, !visivel),
                    |cfg| baixar_planilha(cfg, &dir, None, visivel),
                )
                .await?;
                imprimir_resumo_aguardar(&relatorio);
                registrar_alertas(inicio, |agora, obs| {
                    orquestracao::noites_do_aguardar(&alvos.itens, &relatorio, agora, obs)
                });
                if !relatorio.sucesso() {
                    anyhow::bail!("o ciclo terminou com falha(s); veja o resumo acima");
                }
                return Ok(());
            }
            let relatorio = orquestracao::executar_ciclo(
                &alvos.itens,
                &pastas,
                &mut destino,
                &mut imprimir_evento,
                &mut agora_segundos,
                |cfg| downloader::raspar_ultimo_resultado(cfg, !visivel),
                |cfg| baixar_planilha(cfg, &dir, None, visivel),
            )
            .await;
            imprimir_resumo_ciclo(&relatorio);
            registrar_alertas(inicio, |agora, obs| {
                orquestracao::noites_do_ciclo(&alvos.itens, &relatorio, agora, obs)
            });
            if !relatorio.sucesso() {
                anyhow::bail!("o ciclo terminou com falha(s); veja o resumo acima");
            }
        }
        Comando::Dataset {
            loteria,
            formato,
            trancado,
            saida,
            db,
        } => {
            let cfg = buscar_config_unica(&loteria, "dataset", MOTIVO_DATASET_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            gravar_dataset(cfg, &db_caminho, formato.into(), trancado, &saida)?;
        }
        Comando::Aleatoriedade { loteria, db } => {
            let alvos = resolver_alvos(&loteria, db, None)?;
            let mut execucao = alvos.execucao();
            for (cfg, db_caminho) in alvos.itens {
                execucao.cabecalho(cfg);
                execucao.registrar(cfg, testar_aleatoriedade(cfg, &db_caminho))?;
            }
            execucao.concluir()?;
        }
        Comando::Evidencia {
            loteria,
            perfil,
            db,
            perfis,
        } => {
            let cfg = buscar_config_unica(&loteria, "evidencia", MOTIVO_EVIDENCIA_UMA_LOTERIA)?;
            let db_caminho = db.unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            evidenciar(cfg, &db_caminho, &perfis, perfil.as_deref())?;
        }
        Comando::Alerta { geral } => {
            if geral.trim().is_empty() {
                anyhow::bail!("`--geral` precisa de um motivo (ex.: \"trava ocupada\")");
            }
            let pastas = pastas_de_alertas();
            orquestracao::registrar_falha_geral(
                &pastas,
                &geral,
                &mut agora_segundos,
                &mut imprimir_evento,
            )?;
        }
        Comando::Alertas => listar_alertas()?,
    }

    Ok(())
}

fn pastas_de_alertas() -> orquestracao::PastasAlertas<'static> {
    orquestracao::PastasAlertas {
        alertas: std::path::Path::new(orquestracao::PASTA_ALERTAS),
        logs: std::path::Path::new(log_arquivo::PASTA),
    }
}

/// Fim do `sync ciclo` (3.6): conta a noite de cada loteria, grava os
/// rascunhos devidos e avisa (tela e log) de cada um. `montar` recebe o
/// instante de agora e o observador e devolve o resultado da noite. Nada
/// aqui derruba o ciclo: a falha vira aviso e o código de saída segue o do
/// ciclo.
fn registrar_alertas(
    inicio: i64,
    montar: impl FnOnce(i64, &mut dyn FnMut(orquestracao::Evento)) -> Vec<orquestracao::NoiteAlvo>,
) {
    let agora = match agora_segundos() {
        Ok(s) => i64::try_from(s).unwrap_or(i64::MAX),
        Err(e) => {
            emitir(
                Nivel::Aviso,
                Destino::Erro,
                &format!("Aviso: alertas não registrados, relógio inválido: {e}"),
            );
            return;
        }
    };
    let noites = montar(agora, &mut imprimir_evento);
    let pastas = pastas_de_alertas();
    if let Err(e) = orquestracao::registrar_noite(
        &pastas,
        &noites,
        inicio,
        &mut agora_segundos,
        &mut imprimir_evento,
    ) {
        emitir(
            Nivel::Aviso,
            Destino::Erro,
            &format!("Aviso: alertas não registrados: {e}"),
        );
    }
}

/// `sync alertas`: os rascunhos que ainda não foram movidos para `enviados/`.
fn listar_alertas() -> anyhow::Result<()> {
    let pasta = std::path::Path::new(orquestracao::PASTA_ALERTAS);
    let pendentes = orquestracao::listar_pendentes(pasta)?;
    if pendentes.is_empty() {
        println!("Nenhum rascunho de alerta pendente.");
        return Ok(());
    }
    println!(
        "{} rascunho(s) de alerta pendente(s); depois de enviar, mova cada um para {}/{}/:",
        pendentes.len(),
        orquestracao::PASTA_ALERTAS,
        orquestracao::PASTA_ENVIADOS
    );
    for p in &pendentes {
        println!("  {}", absoluto(&p.caminho).display());
        if let Some(assunto) = &p.assunto {
            println!("    {assunto}");
        }
    }
    Ok(())
}

/// Loterias a processar por um comando, cada uma com seu banco.
struct Alvos {
    /// `true` quando o argumento foi `todas`: decide cabeçalhos e resumo.
    multiplas: bool,
    itens: Vec<(&'static config::LoteriaConfig, PathBuf)>,
}

impl Alvos {
    fn execucao(&self) -> Execucao {
        Execucao::nova(self.multiplas)
    }
}

/// Resolve o argumento `loteria` (slug ou `todas`) nas loterias a processar.
/// Concentra as recusas de opções que só valem com slug único: `--db` (um
/// banco por loteria) e `--seletor` (o seletor é de uma loteria só; só
/// `baixar` o recebe, os demais comandos passam `None`).
fn resolver_alvos(
    loteria: &str,
    db: Option<PathBuf>,
    seletor: Option<&str>,
) -> anyhow::Result<Alvos> {
    let cfgs = config::resolver(loteria).ok_or_else(|| loteria_desconhecida(loteria, true))?;
    let multiplas = config::eh_todas(loteria);
    if multiplas && db.is_some() {
        anyhow::bail!(
            "`--db` não pode ser usado com `{}`: cada loteria tem seu próprio banco \
             (resultados_<loteria>.db); informe o slug para usar outro caminho",
            config::TODAS
        );
    }
    if multiplas && seletor.is_some() {
        anyhow::bail!(
            "`--seletor` não pode ser usado com `{}`: o seletor é de uma \
             loteria só; informe o slug",
            config::TODAS
        );
    }
    let itens = cfgs
        .into_iter()
        .map(|cfg| {
            let caminho = db.clone().unwrap_or_else(|| caminho_db_padrao(cfg.slug));
            (cfg, caminho)
        })
        .collect();
    Ok(Alvos { multiplas, itens })
}

/// Execução de um comando sobre uma ou mais loterias. Com slug único é
/// transparente (o erro sobe como antes); com `todas`, imprime um cabeçalho
/// por loteria, segue adiante quando uma falha e resume no fim.
struct Execucao {
    multiplas: bool,
    situacoes: Vec<(&'static str, bool)>,
}

impl Execucao {
    fn nova(multiplas: bool) -> Self {
        Execucao {
            multiplas,
            situacoes: Vec::new(),
        }
    }

    fn cabecalho(&self, cfg: &config::LoteriaConfig) {
        if self.multiplas {
            println!("=== {} ({}) ===", cfg.nome_exibicao, cfg.slug);
        }
    }

    fn registrar(
        &mut self,
        cfg: &'static config::LoteriaConfig,
        resultado: anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        if !self.multiplas {
            return resultado;
        }
        if let Err(e) = &resultado {
            eprintln!("Erro em {}: {e:#}", cfg.slug);
        }
        self.situacoes.push((cfg.slug, resultado.is_ok()));
        Ok(())
    }

    fn concluir(self) -> anyhow::Result<()> {
        if !self.multiplas {
            return Ok(());
        }
        println!("=== Resumo ===");
        for (slug, ok) in &self.situacoes {
            let situacao = if *ok { "ok" } else { "falhou" };
            println!("  {slug}: {situacao}");
        }
        let falhas: Vec<&str> = self
            .situacoes
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(slug, _)| *slug)
            .collect();
        if !falhas.is_empty() {
            anyhow::bail!(
                "{} de {} loteria(s) falharam: {}",
                falhas.len(),
                self.situacoes.len(),
                falhas.join(", ")
            );
        }
        Ok(())
    }
}

async fn baixar(
    cfg: &'static config::LoteriaConfig,
    dir: &std::path::Path,
    db_caminho: &std::path::Path,
    seletor: Option<&str>,
    visivel: bool,
    cache: &std::path::Path,
) -> anyhow::Result<()> {
    let arquivo = baixar_planilha(cfg, dir, seletor, visivel).await?;
    let hoje = hoje_de(agora_segundos)?;
    orquestracao::processar_e_salvar(&arquivo, cfg, db_caminho, cache, hoje, &mut imprimir_evento)?;
    Ok(())
}

/// Saída do `sync calendario` de uma loteria.
fn imprimir_calendario(
    cfg: &config::LoteriaConfig,
    r: &orquestracao::RelatorioCalendario,
    agora: i64,
) {
    use rust_spider::calendario::{instante_brasilia, EstadoCalendario};
    let dias: Vec<&str> = cfg.dias_sorteio.iter().map(|d| d.nome()).collect();
    println!("Agora: {}", instante_brasilia(agora));
    println!("Dias de sorteio configurados: {}", dias.join(", "));
    let c = &r.consulta;
    match c.procurar_desde {
        Some(desde) => println!(
            "{} ({}): dia de sorteio; procurar o resultado a partir de {}.",
            c.data,
            c.dia_da_semana.nome(),
            instante_brasilia(desde)
        ),
        None => println!(
            "{} ({}): não é dia de sorteio.",
            c.data,
            c.dia_da_semana.nome()
        ),
    }
    println!(
        "Último concurso do banco: {} ({}).",
        r.ultimo_concurso.0, r.ultimo_concurso.1
    );
    let s = &r.situacao;
    match (s.estado, &s.esperado_em, s.atrasado_a_partir_de) {
        (EstadoCalendario::EmDia, _, _) => {
            println!("Estado: em_dia (nenhum concurso esperado faltando).")
        }
        (estado, Some(esperado), Some(limite)) => {
            println!(
                "Estado: {} (o concurso de {esperado} já devia estar no banco; passa a atrasado em {}).",
                estado.nome(),
                instante_brasilia(limite)
            );
            println!("AVISO: feriado, adiamento ou site atrasado são causas possíveis; nada a fazer ainda.");
        }
        (estado, _, _) => println!("Estado: {}.", estado.nome()),
    }
    if r.avisos.is_empty() {
        println!("Conferência do calendário: sem avisos.");
    }
    for aviso in &r.avisos {
        println!("AVISO: {aviso}");
    }
}

/// Resumo do `sync ciclo`: uma linha por loteria e a situação da publicação.
fn imprimir_resumo_ciclo(relatorio: &orquestracao::RelatorioCiclo) {
    println!("=== Resumo do ciclo ===");
    for l in &relatorio.loterias {
        emitir(nivel_do_resumo(l.ok()), Destino::Padrao, &format!("  {l}"));
    }
    match &relatorio.publicacao {
        Ok(_) => println!("  publicação: ok"),
        Err(_) => emitir(Nivel::Erro, Destino::Padrao, "  publicação: falhou"),
    }
}

/// Resumo do `sync ciclo --aguardar`: uma linha por loteria, avisos e as
/// publicações feitas.
fn imprimir_resumo_aguardar(relatorio: &orquestracao::RelatorioAguardar) {
    println!(
        "=== Resumo do ciclo ({} rodada(s), janela até {}) ===",
        relatorio.rodadas,
        rust_spider::calendario::instante_brasilia(relatorio.janela_ate)
    );
    for l in &relatorio.loterias {
        emitir(
            nivel_do_resumo(l.resumo.ok()),
            Destino::Padrao,
            &format!("  {} [{} tentativa(s)]", l.resumo, l.tentativas),
        );
        if let Some(esperado) = &l.concurso_ausente {
            emitir(
                Nivel::Aviso,
                Destino::Padrao,
                &format!(
                    "    AVISO: o concurso do sorteio de {esperado} não saiu até o fim da janela"
                ),
            );
        }
    }
    for p in &relatorio.publicacoes {
        let quem = p.slug.unwrap_or("reenvio");
        match &p.resultado {
            Ok(_) => println!("  publicação ({quem}): ok"),
            Err(_) => emitir(
                Nivel::Erro,
                Destino::Padrao,
                &format!("  publicação ({quem}): falhou"),
            ),
        }
    }
}

/// Nível de uma linha do resumo de loteria: falha é ERRO, o resto INFO.
fn nivel_do_resumo(ok: bool) -> Nivel {
    if ok {
        Nivel::Info
    } else {
        Nivel::Erro
    }
}

/// Mostra um evento dos fluxos de gravação (`orquestracao`) na hora em que
/// acontece, na saída certa (a ordem das mensagens é a de sempre) e o copia
/// no log com o nível do próprio evento.
fn imprimir_evento(evento: orquestracao::Evento) {
    emitir(evento.nivel(), evento.destino(), &evento.to_string());
}

/// Só o download da planilha (sem gravar): a parte do `baixar` que o
/// `atualizar` com salto reaproveita.
async fn baixar_planilha(
    cfg: &'static config::LoteriaConfig,
    dir: &std::path::Path,
    seletor: Option<&str>,
    visivel: bool,
) -> rust_spider::Result<PathBuf> {
    println!("Baixando resultados de {}...", cfg.nome_exibicao);
    let arquivo = downloader::baixar_resultado(cfg, seletor, dir, !visivel).await?;
    println!("Arquivo salvo em {}", arquivo.display());
    Ok(arquivo)
}

fn listar(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
) -> anyhow::Result<Vec<rust_spider::Resultado>> {
    // `db::abrir` cria o arquivo se não existir; listar não deve criar banco
    // vazio (nem responder `[]` como se a loteria não tivesse concursos).
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let conn = db::abrir(db_caminho)?;
    Ok(db::listar(&conn, cfg.slug)?)
}

async fn atualizar(
    cfg: &'static config::LoteriaConfig,
    db_caminho: &std::path::Path,
    downloads: &std::path::Path,
    pastas: &PastasAtualizar<'_>,
    visivel: bool,
) -> anyhow::Result<()> {
    orquestracao::atualizar(
        cfg,
        db_caminho,
        pastas,
        &mut imprimir_evento,
        agora_segundos,
        || downloader::raspar_ultimo_resultado(cfg, !visivel),
        || baixar_planilha(cfg, downloads, None, visivel),
    )
    .await?;
    Ok(())
}

fn validar_banco(cfg: &config::LoteriaConfig, db_caminho: &std::path::Path) -> anyhow::Result<()> {
    // `db::abrir` cria o arquivo se não existir; validar não deve criar banco vazio.
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let conn = db::abrir(db_caminho)?;
    let resultados = db::listar(&conn, cfg.slug)?;

    println!(
        "Validando {} em {}...",
        cfg.nome_exibicao,
        db_caminho.display()
    );
    let (Some(menor), Some(maior)) = (
        resultados.iter().map(|r| r.concurso).min(),
        resultados.iter().map(|r| r.concurso).max(),
    ) else {
        anyhow::bail!("nenhum concurso de {} no banco", cfg.slug);
    };
    println!(
        "{} concurso(s) salvo(s), do {menor} ao {maior}.",
        resultados.len()
    );

    let (erros, avisos) = validacao::validar_separando(cfg, &resultados);

    imprimir_problemas(Nivel::Aviso, &avisos, Destino::Padrao);
    imprimir_problemas(Nivel::Erro, &erros, Destino::Padrao);
    if validacao::tem_buraco(&erros) {
        imprimir_dica_buracos(cfg);
    }

    if erros.is_empty() {
        if avisos.is_empty() {
            println!("Nenhum problema encontrado.");
        } else {
            println!("Nenhum erro encontrado ({} aviso(s)).", avisos.len());
        }
        return Ok(());
    }
    anyhow::bail!(
        "{}: {} erro(s) de integridade em {}",
        cfg.slug,
        erros.len(),
        db_caminho.display()
    )
}

/// Imprime o bloco de avisos (`Nivel::Aviso`) ou de erros (`Nivel::Erro`) da
/// validação, como o `Evento::Avisos`/`Evento::Erros` do mesmo texto. `validar`
/// é um relatório e usa a saída padrão; a recusa de um lote ao gravar é falha
/// do comando e vai para a saída de erro.
fn imprimir_problemas<P: std::fmt::Display>(nivel: Nivel, problemas: &[P], saida: Destino) {
    let rotulo = if nivel == Nivel::Erro {
        "erro(s)"
    } else {
        "aviso(s)"
    };
    for linha in orquestracao::formatar_problemas(rotulo, problemas) {
        emitir(nivel, saida, &linha);
    }
}

/// Para comandos que só fazem sentido com uma loteria (`importar`, `inserir`,
/// `listar`, `features`, `indice`): recusa `todas` explicando o porquê.
fn buscar_config_unica(
    loteria: &str,
    comando: &str,
    motivo: &str,
) -> anyhow::Result<&'static config::LoteriaConfig> {
    if config::eh_todas(loteria) {
        anyhow::bail!(
            "`{comando}` não aceita `{}`: {motivo}. Informe uma de: {}",
            config::TODAS,
            config::slugs_disponiveis()
        );
    }
    config::buscar(loteria).ok_or_else(|| loteria_desconhecida(loteria, false))
}

fn loteria_desconhecida(loteria: &str, aceita_todas: bool) -> anyhow::Error {
    let todas = if aceita_todas {
        format!(" (ou `{}` para todas)", config::TODAS)
    } else {
        String::new()
    };
    anyhow::anyhow!(
        "loteria desconhecida: `{loteria}`. Use uma de: {}{todas}",
        config::slugs_disponiveis()
    )
}

fn caminho_db_padrao(slug: &str) -> PathBuf {
    PathBuf::from(format!("resultados_{slug}.db"))
}

fn imprimir_dica_buracos(cfg: &config::LoteriaConfig) {
    imprimir_evento(orquestracao::Evento::DicaBuracos { slug: cfg.slug });
}

/// Lista cada faixa de concursos faltando com as datas dos vizinhos, que
/// ajudam a localizar o sorteio na fonte oficial.
fn listar_buracos(cfg: &config::LoteriaConfig, db_caminho: &std::path::Path) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let resultados = carregar_existentes(cfg, db_caminho)?;
    let data_de = |concurso: u32| {
        resultados
            .iter()
            .find(|r| r.concurso == concurso)
            .map(|r| r.data_sorteio.as_str())
    };
    let buracos = faixas_faltando(&resultados);
    if buracos.is_empty() {
        println!("{}: nenhum concurso faltando.", cfg.nome_exibicao);
        return Ok(());
    }
    let total: u32 = buracos.iter().map(|(i, f)| f - i + 1).sum();
    println!("{}: {total} concurso(s) faltando.", cfg.nome_exibicao);
    for (inicio, fim) in buracos {
        let faixa = descrever_faixa(inicio, fim);
        let antes = inicio
            .checked_sub(1)
            .and_then(|c| data_de(c).map(|d| format!("depois do {c} ({d})")));
        let depois = data_de(fim + 1).map(|d| format!("antes do {} ({d})", fim + 1));
        let contexto: Vec<String> = [antes, depois].into_iter().flatten().collect();
        if contexto.is_empty() {
            println!("  - {faixa}");
        } else {
            println!("  - {faixa}: {}", contexto.join(", "));
        }
    }
    Ok(())
}

/// Fecha um buraco à mão: valida como qualquer gravação (com data fora de
/// ordem tratada como erro, já que é dado digitado), grava no banco e
/// registra no cache.
fn inserir_manual(
    cfg: &config::LoteriaConfig,
    entrada: manuais::EntradaManual,
    db_caminho: &std::path::Path,
    cache_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let existentes = carregar_existentes(cfg, db_caminho)?;
    if existentes.iter().any(|r| r.concurso == entrada.concurso) {
        anyhow::bail!(
            "o concurso {} de {} já está no banco; `inserir` só fecha buracos",
            entrada.concurso,
            cfg.slug
        );
    }
    let lote = [entrada.para_resultado(cfg.slug)];
    let hoje = hoje_de(agora_segundos)?;
    let decisao = validar_lote(
        cfg,
        &existentes,
        &lote,
        hoje,
        db_caminho,
        &mut imprimir_evento,
    )?;
    if decisao
        .avisos
        .iter()
        .any(|p| matches!(p, validacao::Problema::DataForaDeOrdem { .. }))
    {
        anyhow::bail!(
            "a data {} do concurso {} fica fora de ordem em relação aos vizinhos; \
             confira a data — nada foi gravado",
            entrada.data_sorteio,
            entrada.concurso
        );
    }

    // Cache primeiro: ele é a fonte da verdade do dado digitado. Se o banco
    // falhar depois, a entrada é reaplicada pelo próximo `importar`/`baixar`
    // e um novo `inserir` funciona (substitui a entrada do mesmo concurso).
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let mut entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let concurso = entrada.concurso;
    let data = entrada.data_sorteio.clone();
    manuais::incluir(&mut entradas, entrada);
    manuais::salvar(&cache_caminho, cfg.slug, &entradas)?;

    let mut conn = db::abrir(db_caminho)?;
    db::salvar_resultados(&mut conn, &lote)?;
    println!(
        "Concurso {concurso} ({data}) inserido em {} e registrado em {}.",
        db_caminho.display(),
        cache_caminho.display()
    );
    Ok(())
}

/// `sync features`: valida, calcula e grava (tudo em `orquestracao`) e
/// imprime o resumo. Banco inexistente é erro, sem criar arquivo.
fn calcular_features(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    completo: bool,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let mut conn = db::abrir(db_caminho)?;
    println!(
        "Calculando features de {} em {}...",
        cfg.nome_exibicao,
        db_caminho.display()
    );
    let inicio = std::time::Instant::now();
    let resumo = match orquestracao::sincronizar_features(&mut conn, cfg, completo) {
        Ok(resumo) => resumo,
        Err(SpiderError::ResultadosReprovados {
            loteria,
            erros,
            avisos,
        }) => {
            imprimir_problemas(Nivel::Aviso, &avisos, Destino::Erro);
            imprimir_problemas(Nivel::Erro, &erros, Destino::Erro);
            if validacao::tem_buraco(&erros) {
                imprimir_dica_buracos(cfg);
            }
            anyhow::bail!(SpiderError::ResultadosReprovados {
                loteria,
                erros,
                avisos,
            });
        }
        Err(e) => return Err(e.into()),
    };
    let duracao = inicio.elapsed();

    imprimir_problemas(Nivel::Aviso, &resumo.avisos, Destino::Padrao);
    println!("Modo: {}.", resumo.modo);
    println!(
        "{} concurso(s) incorporado(s); {} linha(s) gravada(s) em features_dezena.",
        resumo.concursos_incorporados, resumo.linhas_gravadas
    );
    if resumo.concursos_incorporados == 0 {
        println!(
            "Já estava em dia até o concurso {}.",
            resumo.ultimo_concurso
        );
    }
    println!(
        "Último concurso: {}; snapshot do próximo ({}) em features_dezena com \
         saiu = NULL. Tempo: {:.2} s.",
        resumo.ultimo_concurso,
        resumo.proximo_concurso,
        duracao.as_secs_f64()
    );
    Ok(())
}

/// `sync indice`: carrega o perfil, calcula e grava o índice (tudo em
/// `orquestracao`) e imprime o resumo. Banco inexistente é erro, sem criar
/// arquivo. É aqui que se lê o relógio para `perfis.criado_em`.
fn calcular_indice(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let criado_em = agora_iso8601_utc()?;
    let mut conn = db::abrir(db_caminho)?;
    let p = &carregado.perfil;
    println!(
        "Calculando o índice de {} em {} com o perfil {} (versão {}, janela {}, {})...",
        cfg.nome_exibicao,
        db_caminho.display(),
        p.nome,
        p.versao,
        descrever_janela(p.janela),
        p.normalizacao.como_str()
    );
    let inicio = std::time::Instant::now();
    let resumo = orquestracao::sincronizar_indice(&mut conn, cfg, &carregado, &criado_em)?;
    let duracao = inicio.elapsed();

    let registro = match resumo.registro {
        orquestracao::RegistroPerfil::Novo => "registrado agora em perfis",
        orquestracao::RegistroPerfil::JaRegistrado => "já registrado em perfis, sem alteração",
    };
    println!(
        "Perfil {}: {registro} (hash {}).",
        resumo.perfil, carregado.hash_toml
    );
    println!(
        "{} concurso(s) indexado(s), do {} ao {}; {} linha(s) gravada(s) em indice \
         ({} substituída(s)).",
        resumo.concursos,
        resumo.primeiro_concurso,
        resumo.proximo_concurso,
        resumo.linhas_gravadas,
        resumo.linhas_apagadas
    );
    println!(
        "Último concurso: {}; índice do próximo ({}) incluído. Tempo: {:.2} s.",
        resumo.ultimo_concurso,
        resumo.proximo_concurso,
        duracao.as_secs_f64()
    );
    Ok(())
}

/// `sync dataset`: lê e confere as features (em `orquestracao`), monta o
/// arquivo e o manifesto (em `dataset`) e grava os dois em
/// `<saida>/<loteria>/`. Banco inexistente é erro, sem criar arquivo; o banco
/// é aberto só para leitura.
fn gravar_dataset(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    formato: dataset::Formato,
    com_trancado: bool,
    saida: &std::path::Path,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let inicio = std::time::Instant::now();
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let ds = orquestracao::dataset_de_features(&mut conn, cfg, com_trancado)?;
    drop(conn);
    let bytes = dataset::montar(&ds, formato)?;
    let manifesto = dataset::manifesto(&ds, formato, &bytes);
    let texto_manifesto = dataset::formatar_manifesto(&manifesto)?;

    let pasta = saida.join(cfg.slug);
    let io = |caminho: &std::path::Path| {
        let caminho = caminho.to_path_buf();
        move |origem| SpiderError::Io { caminho, origem }
    };
    std::fs::create_dir_all(&pasta).map_err(io(&pasta))?;
    // Nomes por modo e formato: com `--trancado`, `features_v<fv>_trancado.<ext>`
    // e `manifesto_trancado_<ext>.json`, então o par da busca nunca é tocado.
    let arquivo = pasta.join(dataset::nome_arquivo(&ds, formato));
    let arquivo_manifesto = pasta.join(dataset::nome_manifesto(&ds, formato));
    // O manifesto antigo do mesmo modo e formato sai antes de o arquivo de
    // dados ser regravado: se a gravação falhar no meio, não sobra manifesto
    // descrevendo outro arquivo. O par do outro modo fica como está.
    match std::fs::remove_file(&arquivo_manifesto) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&arquivo_manifesto)(e).into()),
    }
    std::fs::write(&arquivo, &bytes).map_err(io(&arquivo))?;
    std::fs::write(&arquivo_manifesto, &texto_manifesto).map_err(io(&arquivo_manifesto))?;

    if com_trancado {
        emitir(
            Nivel::Aviso,
            Destino::Erro,
            &format!(
                "Aviso: o dataset inclui o conjunto TRANCADO (concursos acima de {}). Use só na \
                 validação final do perfil campeão, nunca na busca de pesos.",
                cfg.ultimo_concurso_busca
            ),
        );
    }
    println!(
        "Dataset de {} ({}): concursos {} a {} ({} concurso(s); corte da busca {}), {} linha(s), \
         feature_version {}.",
        cfg.nome_exibicao,
        manifesto.trecho,
        manifesto.primeiro_concurso,
        manifesto.ultimo_concurso,
        manifesto.concursos,
        manifesto.ultimo_concurso_busca,
        manifesto.linhas,
        manifesto.feature_version
    );
    println!(
        "Arquivo: {} ({} bytes, SHA-256 {})",
        absoluto(&arquivo).display(),
        bytes.len(),
        manifesto.sha256
    );
    println!(
        "Manifesto: {}. Tempo: {:.2} s.",
        absoluto(&arquivo_manifesto).display(),
        inicio.elapsed().as_secs_f64()
    );
    Ok(())
}

/// `sync exportar`: gera os arquivos de todas as loterias pedidas na memória
/// (só lê os bancos) e só então grava (`orquestracao`). Se uma loteria falhar,
/// nada é gravado.
fn exportar(
    alvos: &Alvos,
    raiz_perfis: &std::path::Path,
    saida: &std::path::Path,
) -> anyhow::Result<()> {
    let inicio = std::time::Instant::now();
    let resultado =
        orquestracao::exportar_com_relatorio(&alvos.itens, raiz_perfis, saida, agora_iso8601_utc)?;
    let relatorio = match resultado {
        orquestracao::ResultadoComRelatorio::Gravado(relatorio) => relatorio,
        orquestracao::ResultadoComRelatorio::Cancelado(mut falhas) => {
            if !alvos.multiplas {
                return Err(falhas.remove(0).erro.into());
            }
            let nomes: Vec<&str> = falhas.iter().map(|f| f.slug).collect();
            for f in falhas {
                eprintln!("Erro em {}: {:#}", f.slug, anyhow::Error::from(f.erro));
            }
            anyhow::bail!(
                "exportação cancelada, nada foi gravado: falhou em {}",
                nomes.join(", ")
            );
        }
    };
    for g in &relatorio.gravados {
        println!(
            "{} ({} bytes, SHA-256 {})",
            absoluto(&g.caminho).display(),
            g.impressao.bytes,
            g.impressao.sha256
        );
    }
    // O ranking por taxa é opcional: omitido não é erro, mas avisa por quê.
    for r in &relatorio.rankings {
        if let orquestracao::SituacaoRanking::Omitido(motivo) = &r.situacao {
            println!("Aviso: {}: ranking_taxa.json omitido: {motivo}.", r.slug);
        }
    }
    for removido in &relatorio.removidos {
        println!(
            "Aviso: ranking_taxa.json antigo removido: {}.",
            absoluto(removido).display()
        );
    }
    // Idem para o G-1 (`ranking_posicoes.json`, tarefa 2.6).
    for p in &relatorio.posicoes {
        if let orquestracao::SituacaoPosicoes::Omitido(motivo) = &p.situacao {
            println!(
                "Aviso: {}: ranking_posicoes.json omitido: {motivo}.",
                p.slug
            );
        }
    }
    for removido in &relatorio.removidos_posicoes {
        println!(
            "Aviso: ranking_posicoes.json antigo removido: {}.",
            absoluto(removido).display()
        );
    }
    println!(
        "Exportação concluída: {} arquivo(s) em {}. Tempo: {:.2} s.",
        relatorio.gravados.len(),
        absoluto(&exportacao::disco::pasta_versao(saida)).display(),
        inicio.elapsed().as_secs_f64()
    );
    Ok(())
}

/// `sync tabela`: monta a tabela geral do concurso (conferências e leitura
/// em `orquestracao`), grava o CSV em `<saida>/<loteria>/` e imprime onde
/// gravou e as 10 primeiras dezenas. Banco inexistente é erro, sem criar
/// arquivo; o banco é aberto só para leitura.
fn gravar_tabela(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
    concurso: Option<u32>,
    saida: &std::path::Path,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let t = orquestracao::tabela_do_concurso(&mut conn, cfg, &carregado, concurso)?;

    let pasta = saida.join(cfg.slug);
    std::fs::create_dir_all(&pasta).map_err(|origem| SpiderError::Io {
        caminho: pasta.clone(),
        origem,
    })?;
    let arquivo = pasta.join(tabela::nome_arquivo(&t));
    std::fs::write(&arquivo, tabela::formatar_csv(&t)).map_err(|origem| SpiderError::Io {
        caminho: arquivo.clone(),
        origem,
    })?;

    let quando = match &t.data_sorteio {
        Some(data) => format!("sorteado em {data}"),
        None => "próximo, ainda não sorteado".to_string(),
    };
    println!(
        "Tabela de {} do concurso {} ({quando}) com o perfil {} (janela {}): {} dezena(s) em {}",
        cfg.nome_exibicao,
        t.concurso,
        t.perfil,
        descrever_janela(t.janela),
        t.linhas.len(),
        absoluto(&arquivo).display()
    );
    let top: Vec<String> = t
        .linhas
        .iter()
        .take(10)
        .map(|l| match l.saiu {
            Some(true) => format!("{:02}*", l.dezena),
            _ => format!("{:02}", l.dezena),
        })
        .collect();
    let legenda = if t.data_sorteio.is_some() {
        " (* = saiu)"
    } else {
        ""
    };
    println!("Top 10: {}{legenda}", top.join(" "));
    Ok(())
}

/// `println!` do relatório do `sync avaliar`, que é montado num `String`
/// antes de o uso do trancado ser gravado (e só depois impresso, sem panic se
/// a saída estiver fechada). Escrever num `String` não falha, por isso o
/// `fmt::Result` é descartado.
macro_rules! linha {
    ($s:expr) => {
        $s.push('\n')
    };
    ($s:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = writeln!($s, $($arg)*);
    }};
}

/// `sync ranking`: calcula e grava as tabelas do ranking por taxa histórica
/// (tudo em `orquestracao`, numa transação), grava o CSV do próximo concurso
/// em `<saida>/<loteria>/` e imprime o relatório. Banco inexistente é erro,
/// sem criar arquivo. O CSV só é gravado depois de as tabelas serem
/// confirmadas; o texto da saída padrão é determinístico (mesma entrada,
/// mesmos bytes) e o tempo de execução vai para a saída de erro.
fn gravar_ranking(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
    saida: &std::path::Path,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let mut conn = db::abrir(db_caminho)?;
    let inicio = std::time::Instant::now();
    let r = orquestracao::sincronizar_ranking(&mut conn, cfg, &carregado)?;
    let duracao = inicio.elapsed();

    let pasta = saida.join(cfg.slug);
    std::fs::create_dir_all(&pasta).map_err(|origem| SpiderError::Io {
        caminho: pasta.clone(),
        origem,
    })?;
    let arquivo = pasta.join(ranking::nome_arquivo(&r));
    std::fs::write(&arquivo, ranking::formatar_csv(&r)).map_err(|origem| SpiderError::Io {
        caminho: arquivo.clone(),
        origem,
    })?;

    let mut texto = String::new();
    linha!(
        texto,
        "Ranking gravado no perfil {} (hash {}, registrado em perfis e conferido contra o .toml): {} linha(s) em \
         ranking_taxa ({} substituída(s)), {} em taxa_posicao e {} em taxa_faixa.",
        r.perfil,
        r.hash_toml,
        r.linhas_gravadas,
        r.linhas_apagadas,
        r.posicoes_gravadas,
        r.faixas_gravadas
    );
    linha!(
        texto,
        "CSV do ranking do concurso {}: {}",
        r.proximo_concurso,
        absoluto(&arquivo).display()
    );
    linha!(texto);
    texto.push_str(&ranking::formatar_relatorio(cfg, &r));
    imprimir(&texto)?;
    let tempo = format!("Tempo: {:.2} s.", duracao.as_secs_f64());
    registrar_no_log(Nivel::Info, &tempo);
    let _ = escrever_stderr(&format!("{tempo}\n"));
    Ok(())
}

/// `sync posicoes`: calcula e grava as tabelas do índice de força das posições
/// (tudo em `orquestracao`, numa transação), grava o CSV do próximo concurso em
/// `<saida>/<loteria>/` e imprime o relatório. Banco inexistente é erro, sem
/// criar arquivo. É aqui que se lê o relógio para `posicao_perfil.criado_em`.
/// O CSV só é gravado depois de as tabelas serem confirmadas; o texto da saída
/// padrão é determinístico e o tempo de execução vai para a saída de erro.
fn gravar_posicoes(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
    nome_perfil_posicoes: &str,
    saida: &std::path::Path,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let forca = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let posicoes = perfil::carregar(raiz_perfis, cfg, nome_perfil_posicoes)?;
    let criado_em = agora_iso8601_utc()?;
    let mut conn = db::abrir(db_caminho)?;
    let inicio = std::time::Instant::now();
    let r = orquestracao::sincronizar_posicoes(&mut conn, cfg, &forca, &posicoes, &criado_em)?;
    let duracao = inicio.elapsed();

    let pasta = saida.join(cfg.slug);
    std::fs::create_dir_all(&pasta).map_err(|origem| SpiderError::Io {
        caminho: pasta.clone(),
        origem,
    })?;
    let arquivo = pasta.join(posicoes_relatorio::nome_arquivo(&r));
    std::fs::write(&arquivo, posicoes_relatorio::formatar_csv(&r)).map_err(|origem| {
        SpiderError::Io {
            caminho: arquivo.clone(),
            origem,
        }
    })?;

    let mut texto = String::new();
    linha!(
        texto,
        "Posições gravadas (perfil de força {}, perfil das posições {}): {} marca(s) e {} feature(s) novas, {} linha(s) em posicao_indice ({} substituída(s)).",
        r.perfil,
        r.perfil_posicoes,
        r.marcas_gravadas,
        r.features_gravadas,
        r.indice_gravado,
        r.indice_apagado
    );
    linha!(
        texto,
        "CSV do índice das posições do concurso {}: {}",
        r.proximo_concurso,
        absoluto(&arquivo).display()
    );
    linha!(texto);
    texto.push_str(&posicoes_relatorio::formatar_relatorio(cfg, &r));
    imprimir(&texto)?;
    let tempo = format!("Tempo: {:.2} s.", duracao.as_secs_f64());
    registrar_no_log(Nivel::Info, &tempo);
    let _ = escrever_stderr(&format!("{tempo}\n"));
    Ok(())
}

/// `sync posicoes --avaliar`: avalia a ordem G-1 contra o acaso (com a nota A
/// da 1f como referência) nas dobras de teste 2 a 5 da busca, sem o
/// aquecimento. Tudo em `orquestracao`, que só lê o banco; aqui o banco é aberto
/// só para leitura (inexistente é erro, sem criar arquivo) e nada é gravado: nem
/// tabelas, nem CSV, nem registro do trancado (que fica fechado). A saída padrão
/// é determinística; o tempo de execução vai para a saída de erro.
fn avaliar_posicoes(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
    nome_perfil_posicoes: &str,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let forca = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let posicoes = perfil::carregar(raiz_perfis, cfg, nome_perfil_posicoes)?;
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let inicio = std::time::Instant::now();
    let r = orquestracao::avaliar_g1_do_banco(&mut conn, cfg, &forca, &posicoes)?;
    let duracao = inicio.elapsed();
    imprimir(&posicoes_relatorio::formatar_avaliacao(cfg, &r))?;
    let tempo = format!("Tempo: {:.2} s.", duracao.as_secs_f64());
    registrar_no_log(Nivel::Info, &tempo);
    let _ = escrever_stderr(&format!("{tempo}\n"));
    Ok(())
}

/// `sync ranking --avaliar`: avalia o ranking por taxa histórica (ordens de A
/// e de B) contra o acaso nas dobras de teste 2 a 5 da busca. Tudo em
/// `orquestracao`, que só lê o banco; aqui o banco é aberto só para leitura
/// (inexistente é erro, sem criar arquivo) e nada é gravado: nem tabelas, nem
/// CSV, nem registro do trancado (que fica fechado). A saída padrão é
/// determinística; o tempo de execução vai para a saída de erro.
fn avaliar_ranking(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let inicio = std::time::Instant::now();
    let r = orquestracao::avaliar_ranking_do_banco(&mut conn, cfg, &carregado)?;
    let duracao = inicio.elapsed();
    imprimir(&ranking::formatar_avaliacao(cfg, &r))?;
    let tempo = format!("Tempo: {:.2} s.", duracao.as_secs_f64());
    registrar_no_log(Nivel::Info, &tempo);
    let _ = escrever_stderr(&format!("{tempo}\n"));
    Ok(())
}

/// Pasta dos registros do trancado: a escolhida (`--registros`, escondida,
/// só para testes) ou a padrão, ao lado do banco.
struct PastaRegistros {
    escolhida: PathBuf,
    padrao: PathBuf,
}

/// `sync avaliar`: carrega o perfil, avalia (tudo em `orquestracao`, que só
/// lê o banco), monta o relatório num `String` antes de o uso do trancado
/// ser gravado e só então o imprime. O banco é aberto só para leitura;
/// inexistente é erro, sem criar arquivo. O relógio é lido aqui, para o
/// registro do trancado.
///
/// Toda a saída padrão é determinística (mesma entrada, mesmos bytes,
/// inclusive o bootstrap); o tempo de execução e os avisos sobre a pasta de
/// registros vão para a saída de erro.
fn avaliar(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    nome_perfil: &str,
    opcoes: &orquestracao::OpcoesAvaliacao,
    pasta_registros: &PastaRegistros,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, nome_perfil)?;
    let avaliado_em = agora_iso8601_utc()?;
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let arquivo_registro = absoluto(&registros::caminho(&pasta_registros.escolhida, cfg.slug));
    if opcoes.trancado {
        avisar_pasta_de_registros(cfg, &arquivo_registro, pasta_registros)?;
        imprimir_nivel(
            Nivel::Aviso,
            &format!(
                "ATENÇÃO: --trancado avalia o conjunto TRANCADO da {} (concursos acima de {}). \
             Ele é de uso único por perfil: se a avaliação der certo, o uso fica registrado \
             em {} (versione no git).\n\n",
                cfg.nome_exibicao,
                cfg.ultimo_concurso_busca,
                arquivo_registro.display()
            ),
        )?;
    }
    let inicio = std::time::Instant::now();
    let (_, texto) = orquestracao::avaliar_perfil_e_formatar(
        &mut conn,
        cfg,
        &carregado,
        opcoes,
        &pasta_registros.escolhida,
        &avaliado_em,
        |r| {
            let mut s = String::new();
            escrever_relatorio(&mut s, cfg, r);
            s
        },
    )?;
    let duracao = inicio.elapsed();
    if let Err(e) = imprimir(&texto) {
        if !opcoes.trancado {
            return Err(e);
        }
        // O uso já conta: o veredito não pode se perder. Tenta a saída de
        // erro (sem panic se também estiver fechada).
        // O relatório já foi copiado no log por `imprimir`; aqui só o motivo.
        let motivo = format!(
            "Não foi possível imprimir o relatório na saída padrão ({e:#}). O uso do \
             trancado JÁ foi registrado em {}; o relatório vai abaixo, na saída de erro.",
            arquivo_registro.display()
        );
        registrar_no_log(Nivel::Erro, &motivo);
        let _ = escrever_stderr(&format!("{motivo}\n\n{texto}"));
        return Err(e.context(format!(
            "o uso do trancado já foi registrado em {}",
            arquivo_registro.display()
        )));
    }
    let tempo = format!("Tempo: {:.2} s.", duracao.as_secs_f64());
    registrar_no_log(Nivel::Info, &tempo);
    let _ = escrever_stderr(&format!("{tempo}\n"));
    Ok(())
}

/// Avisa, na saída de erro, quando o uso do trancado vai para um lugar que
/// pede conferência: o primeiro registro da loteria (o arquivo ainda não
/// existe) ou uma pasta que não é a padrão, ao lado do banco. Mostra sempre
/// o caminho absoluto, para quem rodou de outro diretório perceber.
fn avisar_pasta_de_registros(
    cfg: &config::LoteriaConfig,
    arquivo: &std::path::Path,
    pasta: &PastaRegistros,
) -> anyhow::Result<()> {
    let mut avisos = String::new();
    if absoluto(&pasta.escolhida) != absoluto(&pasta.padrao) {
        linha!(
            avisos,
            "Aviso: pasta de registros FORA DO PADRÃO. O uso do trancado da {} vai ficar \
             gravado em {}, e não em {} (a pasta ao lado do banco, versionada no git): \
             usos registrados em outra pasta não contam para o uso único nem para o \
             Bonferroni.",
            cfg.nome_exibicao,
            arquivo.display(),
            absoluto(&registros::caminho(&pasta.padrao, cfg.slug)).display()
        );
    }
    if !arquivo.exists() {
        linha!(
            avisos,
            "Aviso: primeiro registro do trancado da {}: o arquivo {} ainda não existe e será \
             criado com este uso. Confira se é o registro versionado no git (ele deve estar \
             na pasta `{}` ao lado do banco); se não for, interrompa agora.",
            cfg.nome_exibicao,
            arquivo.display(),
            registros::PASTA_PADRAO
        );
    }
    if avisos.is_empty() {
        return Ok(());
    }
    registrar_no_log(Nivel::Aviso, &avisos);
    escrever_stderr(&avisos)
        .map_err(|e| anyhow::anyhow!("não foi possível escrever o aviso na saída de erro: {e}"))
}

/// Caminho absoluto (sem exigir que exista); se não der, o próprio caminho.
fn absoluto(caminho: &std::path::Path) -> PathBuf {
    std::path::absolute(caminho).unwrap_or_else(|_| caminho.to_path_buf())
}

/// Escreve na saída de erro sem `eprintln!`, que dá panic com a saída
/// fechada. Não registra no log: quem chama decide o texto e o nível.
fn escrever_stderr(texto: &str) -> std::io::Result<()> {
    std::io::Write::write_all(&mut std::io::stderr().lock(), texto.as_bytes())
}

/// [`imprimir_nivel`] com nível INFO.
fn imprimir(texto: &str) -> anyhow::Result<()> {
    imprimir_nivel(Nivel::Info, texto)
}

/// Escreve na saída padrão sem `println!`, que dá panic com a saída fechada
/// (`| head`, por exemplo): aqui vira um erro comum. Copia no log antes de
/// escrever, para o texto constar mesmo se a saída estiver fechada.
fn imprimir_nivel(nivel: Nivel, texto: &str) -> anyhow::Result<()> {
    use std::io::Write as _;
    registrar_no_log(nivel, texto);
    let mut saida = std::io::stdout().lock();
    saida
        .write_all(texto.as_bytes())
        .and_then(|()| saida.flush())
        .map_err(|e| anyhow::anyhow!("não foi possível escrever na saída padrão: {e}"))
}

fn escrever_relatorio(
    s: &mut String,
    cfg: &config::LoteriaConfig,
    r: &orquestracao::RelatorioAvaliacao,
) {
    let faixa = r.veredito.avaliacao.faixa;
    linha!(
        s,
        "Avaliação de {} ({}) com o perfil {} (versão {}, janela {}, {}).",
        cfg.nome_exibicao,
        r.loteria,
        r.perfil,
        r.versao,
        descrever_janela(r.janela),
        r.normalizacao
    );
    linha!(s, "  Hash do perfil: {}", r.hash_toml);
    if let orquestracao::TrechoAvaliado::Trancado(uso) = &r.trecho {
        // Sempre, em toda execução com --trancado: qual arquivo foi lido
        // (caminho absoluto) e quantos usos ele tinha. É a leitura sobre a
        // qual este uso é gravado e o Bonferroni é contado.
        let conteudo = if uso.arquivo_existia {
            format!(
                "{} uso(s) registrado(s) antes deste, {} deste perfil",
                uso.usos_lidos,
                uso.anteriores.len()
            )
        } else {
            "0 (arquivo inexistente; é criado com este uso)".to_string()
        };
        linha!(
            s,
            "  Registro do trancado lido: {}; usos: {conteudo}.",
            uso.arquivo.display()
        );
    }
    let linhas: Vec<(String, &AvaliacaoFaixa)> = match &r.trecho {
        orquestracao::TrechoAvaliado::Busca(wf) => {
            linha!(s,
                "  Trecho: busca, walk-forward em {} dobras consecutivas; concursos {} ({} avaliados).",
                wf.dobras.len(),
                descrever_faixa(faixa.primeiro, faixa.ultimo),
                faixa.tamanho()
            );
            linha!(s,
                "  Corte da busca: concurso {} (o trancado, do {} em diante, fica fora desta avaliação).",
                r.corte,
                u64::from(r.corte) + 1
            );
            wf.dobras
                .iter()
                .enumerate()
                .map(|(i, d)| ((i + 1).to_string(), d))
                .chain(std::iter::once((
                    "total".to_string(),
                    &r.veredito.avaliacao,
                )))
                .collect()
        }
        orquestracao::TrechoAvaliado::Trancado(_) => {
            linha!(
                s,
                "  Trecho: TRANCADO; concursos {} ({} avaliados).",
                descrever_faixa(faixa.primeiro, faixa.ultimo),
                faixa.tamanho()
            );
            linha!(
                s,
                "  Corte da busca: concurso {} (o trancado vai do {} até o último sorteado).",
                r.corte,
                u64::from(r.corte) + 1
            );
            // O veredito do trancado cobre exatamente o trancado inteiro.
            vec![("trancado".to_string(), &r.veredito.avaliacao)]
        }
    };
    linha!(
        s,
        "  Índice: concursos {} (último sorteado {} e o snapshot do próximo); a calibração começa \
         no {} com m = {} concursos fictícios, e esse primeiro concurso não é avaliado.",
        descrever_faixa(r.primeiro_indice, r.ultimo_concurso + 1),
        r.ultimo_concurso,
        r.primeiro_indice,
        avaliacao::CONCURSOS_FICTICIOS
    );
    let padrao = if r.k as usize == cfg.qtd_dezenas {
        " (padrão: K da loteria)"
    } else {
        ""
    };
    linha!(s, "  Top-k: k = {}{padrao}.", r.k);

    linha!(s);
    linha!(
        s,
        "Log loss (menor é melhor; diferença < 0 = perfil melhor que o acaso):"
    );
    escrever_tabela_metrica(s, &linhas_da_metrica(&linhas, |a| &a.metricas.log_loss));
    linha!(s);
    linha!(
        s,
        "Brier (menor é melhor; diferença < 0 = perfil melhor que o acaso):"
    );
    escrever_tabela_metrica(s, &linhas_da_metrica(&linhas, |a| &a.metricas.brier));
    linha!(s);
    if let Some(linear) = &r.linear {
        escrever_log_loss_linear(s, linear);
        linha!(s);
    }
    let topk = &r.veredito.avaliacao.topk;
    linha!(
        s,
        "Top-{} (acertos por concurso; maior é melhor; ao acaso k·K/N = {:.4}):",
        r.k,
        topk.acertos.acaso
    );
    escrever_tabela_topk(s, &linhas);
    linha!(s);
    linha!(
        s,
        "Distribuição de acertos do top-{} no trecho inteiro (concursos; observado × acaso):",
        r.k
    );
    for f in &topk.distribuicao {
        linha!(
            s,
            "  {:>2} acerto(s): {:>6} × {:>9.1}",
            f.acertos,
            f.observados,
            f.esperados
        );
    }
    linha!(s);
    escrever_veredito(s, r);
    linha!(s);
    match &r.trecho {
        orquestracao::TrechoAvaliado::Busca(_) => {
            linha!(
                s,
                "Aviso: os p-valores só valem se o perfil e o k foram escolhidos sem ver os \
                 sorteios deste trecho. Se a busca de pesos usou estes concursos, ou se este é o \
                 melhor entre vários perfis ou k, eles saem otimistas: informe quantos foram \
                 testados com --testados N (Bonferroni) ou avalie o campeão no conjunto trancado \
                 (--trancado, uso único)."
            );
            linha!(
                s,
                "Aviso: o IC normal e o EP das tabelas ignoram a correlação serial da calibração \
                 acumulada (tendem a sair estreitos); o veredito usa o bootstrap por blocos."
            );
        }
        orquestracao::TrechoAvaliado::Trancado(uso) => {
            linha!(
                s,
                "Registro do trancado: uso gravado em {} (perfil {}, hash {}, até o concurso {}, \
                 em {}).",
                uso.arquivo.display(),
                uso.registrada.perfil,
                uso.registrada.hash_toml,
                uso.registrada.ate_concurso,
                uso.registrada.avaliado_em
            );
            if !uso.anteriores.is_empty() {
                linha!(
                    s,
                    "Avaliações anteriores deste perfil no trancado (--repetir; cada olhada a \
                     mais enfraquece o trancado como teste independente):"
                );
                for a in &uso.anteriores {
                    linha!(
                        s,
                        "  - em {}, até o concurso {}",
                        a.avaliado_em,
                        a.ate_concurso
                    );
                }
            }
        }
    }
}

/// Uma linha de tabela de log loss ou Brier: rótulo, faixa de concursos, T e
/// a comparação perfil × acaso.
struct LinhaMetrica<'a> {
    rotulo: String,
    faixa: avaliacao::FaixaConcursos,
    concursos: usize,
    comparacao: &'a Comparacao,
}

fn linhas_da_metrica<'a>(
    linhas: &[(String, &'a AvaliacaoFaixa)],
    metrica: fn(&AvaliacaoFaixa) -> &Comparacao,
) -> Vec<LinhaMetrica<'a>> {
    linhas
        .iter()
        .map(|(rotulo, a)| LinhaMetrica {
            rotulo: rotulo.clone(),
            faixa: a.faixa,
            concursos: a.metricas.concursos_avaliados,
            comparacao: metrica(a),
        })
        .collect()
}

/// Log loss com a calibração linear (1e): mesmas dobras e mesmo total do
/// walk-forward. Só informativo.
fn escrever_log_loss_linear(s: &mut String, linear: &avaliacao::LogLossLinear) {
    linha!(
        s,
        "Log loss com a calibração linear na posição (só informativo: o veredito, o \
         bootstrap, o top-k e as tabelas acima seguem com a calibração pela \
         posição; diferença < 0 = perfil melhor que o acaso):"
    );
    let linhas: Vec<LinhaMetrica> = linear
        .dobras
        .iter()
        .enumerate()
        .map(|(i, d)| ((i + 1).to_string(), d))
        .chain(std::iter::once(("total".to_string(), &linear.total)))
        .map(|(rotulo, d)| LinhaMetrica {
            rotulo,
            faixa: avaliacao::FaixaConcursos {
                primeiro: d.primeiro_concurso,
                ultimo: d.ultimo_concurso,
            },
            concursos: d.concursos_avaliados,
            comparacao: &d.log_loss,
        })
        .collect();
    escrever_tabela_metrica(s, &linhas);
    linha!(
        s,
        "  Calibração linear: p = K/N + b·z(posição), um parâmetro b estimado só com o \
         passado (m = {} concursos fictícios com b = 0); ln FB de B no \
         `sync evidencia` = −pares × a diferença das dobras 2 a 5.",
        avaliacao::CONCURSOS_FICTICIOS
    );
}

/// Uma tabela de log loss ou Brier: uma linha por dobra (ou o trancado).
fn escrever_tabela_metrica(s: &mut String, linhas: &[LinhaMetrica]) {
    linha!(
        s,
        "  {:>8}  {:>11}  {:>5}  {:>10}  {:>10}  {:>11}  {:>10}  IC 95% (normal)",
        "dobra",
        "concursos",
        "T",
        "perfil",
        "acaso",
        "diferença",
        "EP"
    );
    for l in linhas {
        let c = l.comparacao;
        linha!(
            s,
            "  {:>8}  {:>11}  {:>5}  {:>10.6}  {:>10.6}  {:>11}  {:>10}  {}",
            l.rotulo,
            descrever_faixa(l.faixa.primeiro, l.faixa.ultimo),
            l.concursos,
            c.perfil,
            c.acaso,
            format!("{:+.3e}", c.diferenca),
            c.erro_padrao
                .map_or_else(|| "—".to_string(), |ep| format!("{ep:.3e}")),
            c.ic95.map_or_else(
                || "—".to_string(),
                |ic| formatar_intervalo(ic, |x| format!("{x:+.3e}"))
            )
        );
    }
}

/// A tabela do top-k: acertos por concurso e o p-valor exato da 1c.2.
fn escrever_tabela_topk(s: &mut String, linhas: &[(String, &AvaliacaoFaixa)]) {
    linha!(
        s,
        "  {:>8}  {:>11}  {:>5}  {:>7}  {:>7}  {:>9}  {:>7}  {:>19}  p exato",
        "dobra",
        "concursos",
        "T",
        "perfil",
        "acaso",
        "diferença",
        "EP",
        "IC 95% (normal)"
    );
    for (rotulo, a) in linhas {
        let c = &a.topk.acertos;
        linha!(
            s,
            "  {:>8}  {:>11}  {:>5}  {:>7.4}  {:>7.4}  {:>9}  {:>7}  {:>19}  {}",
            rotulo,
            descrever_faixa(a.faixa.primeiro, a.faixa.ultimo),
            a.topk.concursos_avaliados,
            c.perfil,
            c.acaso,
            format!("{:+.4}", c.diferenca),
            c.erro_padrao
                .map_or_else(|| "—".to_string(), |ep| format!("{ep:.4}")),
            c.ic95.map_or_else(
                || "—".to_string(),
                |ic| formatar_intervalo(ic, |x| format!("{x:+.4}"))
            ),
            formatar_cauda(a.topk.p_valor)
        );
    }
}

fn escrever_veredito(s: &mut String, r: &orquestracao::RelatorioAvaliacao) {
    let v = &r.veredito;
    let faixa = v.avaliacao.faixa;
    linha!(
        s,
        "Veredito no trecho inteiro (concursos {}, {} avaliados):",
        descrever_faixa(faixa.primeiro, faixa.ultimo),
        faixa.tamanho()
    );
    match &v.bootstrap {
        None => {
            linha!(s,
            "  Sem bootstrap: o trecho tem {} concursos, menos que os {} (dois blocos de {}) que \
             o bootstrap por blocos exige.",
            faixa.tamanho(),
            v.concursos_minimos,
            avaliacao::BLOCO_BOOTSTRAP
        )
        }
        Some(b) => {
            linha!(
                s,
                "  Bootstrap circular por blocos de {} concursos, {} reamostragens, semente {}.",
                b.log_loss.bloco,
                b.log_loss.reamostragens,
                b.log_loss.semente
            );
            linha!(
                s,
                "  Log loss, perfil − acaso: média {:+.3e}, IC 95% percentil {}, p unilateral \
                 (perfil melhor) = {}.",
                b.log_loss.media,
                formatar_intervalo(b.log_loss.ic95, |x| format!("{x:+.3e}")),
                formatar_p(b.log_loss.p_valor)
            );
            linha!(
                s,
                "  Brier (só informativo): média {:+.3e}, IC 95% percentil {}, p = {}.",
                b.brier.media,
                formatar_intervalo(b.brier.ic95, |x| format!("{x:+.3e}")),
                formatar_p(b.brier.p_valor)
            );
        }
    }
    let topk = &v.avaliacao.topk;
    linha!(
        s,
        "  Top-{} (p exato unilateral, trecho inteiro): {} acertos em {} concursos (ao acaso, \
         {:.1} em média), p = {}.",
        r.k,
        topk.total_acertos,
        topk.concursos_avaliados,
        topk.acertos.acaso * topk.concursos_avaliados as f64,
        formatar_cauda(topk.p_valor)
    );
    if let Some(c) = &r.bonferroni {
        let origem = match r.trecho {
            orquestracao::TrechoAvaliado::Busca(_) => "perfis/k testados (--testados)",
            orquestracao::TrechoAvaliado::Trancado(_) => {
                "perfis distintos que já usaram o trancado, contando este"
            }
        };
        linha!(
            s,
            "  Bonferroni, N = {} {origem}: log loss p = {}; top-{} p = {}.",
            c.comparacoes,
            c.p_log_loss.map_or_else(|| "—".to_string(), formatar_p),
            r.k,
            formatar_cauda(c.p_topk)
        );
    }
    let alfa = avaliacao::ALFA;
    let decisao = match (&v.bootstrap, &r.bonferroni) {
        (None, _) => None,
        (Some(b), Some(c)) => c.p_log_loss.map(|p| {
            (
                p <= alfa,
                format!(
                    "p corrigido = {} (sem correção: {})",
                    formatar_p(p),
                    formatar_p(b.log_loss.p_valor)
                ),
            )
        }),
        (Some(b), None) => Some((
            b.log_loss.p_valor <= alfa,
            format!("p = {}", formatar_p(b.log_loss.p_valor)),
        )),
    };
    match decisao {
        None => linha!(s, "=> SEM VEREDITO: trecho curto demais para o bootstrap."),
        Some((true, p)) => {
            linha!(s, "=> SUPERA o acaso no log loss: {p} ≤ α = {alfa}.");
            linha!(
                s,
                "   Ganho sobre sorteios independentes é suspeito por princípio: confira \
                 vazamento (ponto no tempo) e sobreajuste antes de acreditar."
            );
        }
        Some((false, p)) => linha!(s, "=> NÃO supera o acaso no log loss: {p} > α = {alfa}."),
    }
}

fn formatar_intervalo(ic: Intervalo, numero: impl Fn(f64) -> String) -> String {
    format!("[{}, {}]", numero(ic.inferior), numero(ic.superior))
}

/// p-valor com 4 casas, ou em notação científica abaixo de 0,001.
fn formatar_p(p: f64) -> String {
    if p >= 1e-3 {
        format!("{p:.4}")
    } else {
        format!("{p:.3e}")
    }
}

/// Cauda do top-k: como [`formatar_p`], ou pela potência de 10 quando a
/// probabilidade é pequena demais para o `f64`.
fn formatar_cauda(c: Cauda) -> String {
    if c.probabilidade >= f64::MIN_POSITIVE {
        formatar_p(c.probabilidade)
    } else {
        format!("10^{:.1}", c.log10)
    }
}

/// `sync aleatoriedade`: qui-quadrado e runs test por dezena com Holm sobre
/// o histórico inteiro (em `orquestracao`) e o relatório. Banco aberto só
/// para leitura; inexistente é erro, sem criar arquivo.
fn testar_aleatoriedade(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let conn = db::abrir_somente_leitura(db_caminho)?;
    let inicio = std::time::Instant::now();
    let r = orquestracao::testar_aleatoriedade_do_banco(&conn, cfg)?;
    let duracao = inicio.elapsed();

    println!(
        "Testes de aleatoriedade de {} ({}): concursos {} ({} concursos, o histórico inteiro, \
         inclusive o trancado), α = {}.",
        cfg.nome_exibicao,
        cfg.slug,
        descrever_faixa(r.primeiro_concurso, r.ultimo_concurso),
        r.concursos,
        r.alfa
    );
    let q = &r.qui_quadrado;
    println!();
    println!("Qui-quadrado de frequência (corrigido para sorteio sem reposição; teste global):");
    println!(
        "  estatística {:.4} com {} graus de liberdade, p = {} → {}.",
        q.estatistica,
        q.graus_liberdade,
        formatar_p(q.p_valor),
        if q.rejeita {
            "REJEITA a uniformidade (p ≤ α)"
        } else {
            "não rejeita a uniformidade (p > α)"
        }
    );
    println!(
        "  Esperado por dezena: {:.2} saídas; Pearson sem a correção (só referência): {:.4}.",
        q.esperado, q.pearson
    );
    println!();
    println!(
        "Runs test por dezena (p exato bilateral pelo dobro da menor cauda; Holm sobre as {} \
         dezenas):",
        r.dezenas_testadas_runs
    );
    println!(
        "  {:>6}  {:>7}  {:>10}  {:>9}  {:>10}  {:>10}",
        "dezena", "saídas", "sequências", "média", "p", "p Holm"
    );
    for d in &r.runs {
        let t = &d.teste;
        let marca = if d.rejeita {
            "  REJEITA"
        } else if t.fixa() {
            "  fixa (p = 1)"
        } else {
            ""
        };
        println!(
            "  {:>6}  {:>7}  {:>10}  {:>9.1}  {:>10}  {:>10}{marca}",
            format!("{:02}", d.dezena),
            t.saidas,
            t.sequencias,
            t.media,
            formatar_p(t.p_valor),
            formatar_p(d.p_valor_holm)
        );
    }
    let rejeitadas: Vec<String> = r
        .runs
        .iter()
        .filter(|d| d.rejeita)
        .map(|d| format!("{:02}", d.dezena))
        .collect();
    println!(
        "  {} dezena(s) testada(s), {} fixa(s) (nunca ou sempre saíram); {} com p ≤ α sem \
         correção; {} rejeitada(s) depois do Holm{}.",
        r.dezenas_testadas_runs,
        r.dezenas_fixas_runs,
        r.rejeicoes_runs_sem_correcao,
        r.rejeicoes_runs,
        if rejeitadas.is_empty() {
            String::new()
        } else {
            format!(": {}", rejeitadas.join(", "))
        }
    );
    if q.rejeita || r.rejeicoes_runs > 0 {
        println!(
            "Rejeição: confira primeiro os dados (sync validar; erros da planilha), depois o \
             sorteio."
        );
    }
    emitir(
        Nivel::Info,
        Destino::Erro,
        &format!("Tempo: {:.2} s.", duracao.as_secs_f64()),
    );
    Ok(())
}

/// `sync evidencia`: evidência bayesiana por dezena (A) e do índice de força
/// (B), em `orquestracao`, e o relatório. Banco aberto só para leitura;
/// inexistente é erro, sem criar arquivo. O tempo vai para o stderr: o stdout
/// é determinístico.
///
/// `perfil` é o `--perfil`: sem ele vale o perfil ativo da loteria
/// (`cfg.perfil_ativo`) e, se B não puder ser calculada, o relatório sai com o
/// motivo e o comando termina com sucesso; com `--perfil` explícito a mesma
/// falha também imprime o relatório (A inclusive) mas termina com erro.
fn evidenciar(
    cfg: &config::LoteriaConfig,
    db_caminho: &std::path::Path,
    raiz_perfis: &std::path::Path,
    perfil: Option<&str>,
) -> anyhow::Result<()> {
    if !db_caminho.exists() {
        anyhow::bail!("banco não encontrado: {}", db_caminho.display());
    }
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let nome_perfil = perfil.unwrap_or(cfg.perfil_ativo);
    let inicio = std::time::Instant::now();
    let relatorio = match orquestracao::evidencia_completa(&mut conn, cfg, raiz_perfis, nome_perfil)
    {
        Ok(r) => r,
        Err(SpiderError::ResultadosReprovados {
            loteria,
            erros,
            avisos,
        }) => {
            imprimir_problemas(Nivel::Aviso, &avisos, Destino::Erro);
            imprimir_problemas(Nivel::Erro, &erros, Destino::Erro);
            if validacao::tem_buraco(&erros) {
                imprimir_dica_buracos(cfg);
            }
            anyhow::bail!(SpiderError::ResultadosReprovados {
                loteria,
                erros,
                avisos,
            });
        }
        Err(e) => return Err(e.into()),
    };
    let duracao = inicio.elapsed();
    imprimir_problemas(Nivel::Aviso, &relatorio.dezenas.avisos, Destino::Erro);
    let mut texto = formatar_evidencia(&relatorio.dezenas);
    texto.push('\n');
    texto.push_str(&formatar_evidencia_indice(
        &relatorio.indice,
        perfil.is_some(),
    ));
    imprimir(&texto)?;
    emitir(
        Nivel::Info,
        Destino::Erro,
        &format!("Tempo: {:.2} s.", duracao.as_secs_f64()),
    );
    if let (orquestracao::SecaoIndice::Indisponivel(i), Some(_)) = (&relatorio.indice, perfil) {
        anyhow::bail!(
            "a evidência do índice (B) não pôde ser calculada com o perfil `{}` pedido em \
             `--perfil` (A foi impressa): {}",
            i.perfil,
            i.motivo
        );
    }
    Ok(())
}

/// Seção B do relatório do `sync evidencia`: o FB prequencial do índice nas
/// dobras de teste 2 a 5, ou o motivo de não ter sido calculado. `explicito`:
/// o perfil veio de `--perfil` (senão é o perfil ativo da loteria).
fn formatar_evidencia_indice(secao: &orquestracao::SecaoIndice, explicito: bool) -> String {
    let mut s = String::new();
    let mut linha = |texto: String| {
        s.push_str(&texto);
        s.push('\n');
    };
    linha("== Evidência do índice de força (B) ==".to_string());
    let origem = if explicito {
        "informado por --perfil"
    } else {
        "perfil ativo da loteria"
    };
    match secao {
        orquestracao::SecaoIndice::Indisponivel(i) => {
            linha(format!("Perfil: {} ({origem}).", i.perfil));
            linha(format!("B não pôde ser calculada: {}", i.motivo));
            if !i.comandos.is_empty() {
                linha("Comandos a rodar, nesta ordem:".to_string());
                for comando in &i.comandos {
                    linha(format!("  {comando}"));
                }
            }
            linha("A seção A acima não depende do perfil e vale do mesmo jeito.".to_string());
        }
        orquestracao::SecaoIndice::Calculada(c) => {
            let b = &c.evidencia;
            linha(format!(
                "Perfil: {} (versão {}, janela {}, {}; {origem}).",
                c.perfil,
                c.versao,
                descrever_janela(c.janela),
                c.normalizacao
            ));
            linha(format!("  Hash do perfil: {}", c.hash_toml));
            linha(format!(
                "  Índice lido: concursos {} (até o último sorteado + 1); só o trecho abaixo \
                 entra no cálculo.",
                descrever_faixa(c.primeiro_indice, c.ultimo_concurso + 1)
            ));
            linha(format!(
                "  Trecho: dobras de teste 2 a 5 do walk-forward de 5 dobras (as mesmas do \
                 `sync avaliar`; a dobra 1 só treina na busca de pesos e fica fora); concursos \
                 {} ({} concursos, {} pares concurso-dezena).",
                descrever_faixa(b.faixa.primeiro, b.faixa.ultimo),
                b.concursos,
                b.pares
            ));
            linha(format!(
                "  Corte da busca: concurso {} (o trancado, do {} em diante, não entra em B).",
                c.corte,
                u64::from(c.corte) + 1
            ));
            linha(format!(
                "  Probabilidades (régua principal): calibração LINEAR na posição do \
                 índice, p = K/N + b·z(posição), um parâmetro b estimado só com o \
                 passado (m = {} concursos fictícios com b = 0), contra o acaso K/N, \
                 ambas com p limitado a [{:e}, 1 − {:e}] como no log loss.",
                avaliacao::CONCURSOS_FICTICIOS,
                avaliacao::LIMITE_PROBABILIDADE,
                avaliacao::LIMITE_PROBABILIDADE
            ));
            linha(String::new());
            linha(format!(
                "  log10 FB (índice contra o acaso, prequencial, calibração linear): {:+.3}  →  grau {}, {}.",
                b.log10_fb,
                b.grau.nome(),
                b.direcao.nome()
            ));
            linha(format!(
                "  ln FB = {:+.6}; equivale a uma diferença de log loss de {:+.3e} \
                 por par (perfil − acaso; < 0 = perfil melhor), a mesma da linha de \
                 log loss com a calibração linear do `sync avaliar` nas dobras 2 a 5, \
                 em outra escala.",
                b.ln_fb,
                -b.ln_fb / b.pares as f64
            ));
            linha(
                "  FB > 1: a favor do índice; FB ≤ 1 (FB = 1 exato inclusive): a \
                 favor do acaso. Graus como em A. Ao contrário de A, aqui a favor do \
                 acaso não há teto: o índice calibrado é um modelo concreto, que pode \
                 perder para o acaso por muito."
                    .to_string(),
            );
            let inerte = &c.inerte;
            linha(String::new());
            linha(format!(
                "  Esperado de um índice inerte, na mesma calibração linear (as \
                 mesmas posições diante de {} históricos sorteados ao acaso, semente \
                 {}): log10 FB médio {:+.3}; 95 % das réplicas entre {:+.3} e \
                 {:+.3}.",
                inerte.replicas,
                inerte.semente,
                inerte.media_log10,
                inerte.p025_log10,
                inerte.p975_log10
            ));
            linha(format!(
                "  Observado contra o inerte: {:+.3} ({:+.3} em relação à média); {} \
                 de {} réplicas inertes deram log10 FB igual ou maior.",
                b.log10_fb,
                b.log10_fb - inerte.media_log10,
                inerte.replicas_iguais_ou_acima,
                inerte.replicas
            ));
            linha(
                "  ATENÇÃO: mesmo um índice sem informação paga um custo para \
                 aprender a calibração. Na régua linear (um parâmetro) esse custo é \
                 pequeno, da ordem de ½·ln T; mesmo assim o grau acima não quer \
                 dizer, sozinho, que o índice atrapalha: o que mostra se ele carrega \
                 informação é o observado ficar acima da faixa dos inertes."
                    .to_string(),
            );
            let por_posicao = &c.por_posicao;
            linha(String::new());
            linha(format!(
                "  Informativo, sem inerte — calibração pela posição da 1c (uma taxa \
                 por posição, m = {} concursos fictícios): log10 FB {:+.3}  →  grau \
                 {}, {}.",
                avaliacao::CONCURSOS_FICTICIOS,
                por_posicao.log10_fb,
                por_posicao.grau.nome(),
                por_posicao.direcao.nome()
            ));
            linha(
                "  Essa régua estima N taxas e paga um custo de aprendizado grande \
                 mesmo para um índice inerte (medido na 1d.3: ln FB médio de cerca de \
                 −17, −40 e −60 na Lotofácil, Mega-Sena e Quina), então o seu grau \
                 não decide nada; a linha existe para comparar com a do `sync \
                 avaliar` e com a edição anterior deste relatório."
                    .to_string(),
            );
            linha(String::new());
            linha("  Por dobra (só informativo; o grau sai do total):".to_string());
            linha(format!(
                "    {:>5}  {:>11}  {:>9}  {:>8}  {:>10}  {:>14}  {:>10}  {:>14}",
                "dobra",
                "concursos",
                "avaliados",
                "pares",
                "log10 FB",
                "Δ log loss/par",
                "FB posição",
                "Δ posição/par"
            ));
            for (d, p) in b.por_dobra.iter().zip(&por_posicao.por_dobra) {
                linha(format!(
                    "    {:>5}  {:>11}  {:>9}  {:>8}  {:>+10.3}  {:>+14.3e}  {:>+10.3}  {:>+14.3e}",
                    d.dobra,
                    descrever_faixa(d.faixa.primeiro, d.faixa.ultimo),
                    d.concursos,
                    d.pares,
                    d.log10_fb,
                    -d.ln_fb / d.pares as f64,
                    p.log10_fb,
                    -p.ln_fb / p.pares as f64
                ));
            }
            linha(
                "    log10 FB e Δ log loss/par: calibração linear (a régua \
                 principal). Δ log loss/par = log loss do perfil − log loss do acaso \
                 na dobra (< 0 = perfil melhor), a mesma diferença que o `sync \
                 avaliar` mostra por dobra na linha linear. FB posição e Δ \
                 posição/par: o mesmo com a calibração pela posição, que bate com as \
                 tabelas de log loss do `sync avaliar`."
                    .to_string(),
            );
            linha(String::new());
            linha(
                "  Ressalva: as dezenas de um concurso são tratadas como Bernoullis \
                 independentes (pseudo-verossimilhança), igual para o índice e para o acaso; \
                 na verdade elas somam exatamente K. O FB é uma régua de evidência, não a \
                 probabilidade exata do sorteio."
                    .to_string(),
            );
            linha(
                "  Ressalva: se o perfil foi escolhido ou ajustado vendo estes concursos (busca de \
                 pesos), o FB sai otimista; o conjunto trancado existe para isso."
                    .to_string(),
            );
        }
    }
    s
}

/// Percentual com uma casa (`0.025` vira `2.5 %`).
fn formatar_desvio(desvio: f64) -> String {
    format!("{:.1} %", desvio * 100.0)
}

/// Relatório em texto do `sync evidencia`. Determinístico: mesma entrada,
/// mesmos bytes (formatação de floats fixa, dezenas em ordem crescente).
fn formatar_evidencia(r: &orquestracao::RelatorioEvidencia) -> String {
    let mut s = String::new();
    let mut linha = |texto: String| {
        s.push_str(&texto);
        s.push('\n');
    };

    let proximo = r.ultimo_concurso + 1;
    let p = r.qtd_dezenas as f64 / f64::from(r.dezenas_possiveis);
    linha(format!(
        "Evidência bayesiana por dezena de {} ({}): para o concurso {proximo}, só com os \
         concursos até o {}.",
        r.nome_exibicao, r.loteria, r.ultimo_concurso
    ));
    linha(format!(
        "H0: cada dezena sai com chance exatamente K/N = {}/{} = {p:.6}. H1: a chance tem priori \
         Beta centrada em K/N.",
        r.qtd_dezenas, r.dezenas_possiveis
    ));
    if let Some(j) = r.janelas.first() {
        let prioris: Vec<String> = j
            .evidencia
            .prioris
            .iter()
            .map(|q| {
                format!(
                    "{} (a = {:.2}, b = {:.2})",
                    formatar_desvio(q.desvio),
                    q.a,
                    q.b
                )
            })
            .collect();
        linha(format!(
            "Priori: desvio padrão da chance sob H1 = s·K/N; principal s = {}; sensibilidade: {}.",
            prioris[0],
            prioris[1..].join(" e ")
        ));
    }
    linha(
        "Fator de Bayes (FB) em log10; grau sobre F = max(FB, 1/FB): muito fraca F < 3, fraca \
         3 ≤ F < 10, moderada 10 ≤ F < 30, forte 30 ≤ F < 100, muito forte F ≥ 100. Direção: \
         a favor de viés se FB > 1; a favor do acaso se FB < 1 ou FB = 1."
            .to_string(),
    );
    linha(
        "ASSIMETRIA: a favor do acaso a evidência tem teto, porque H0 (chance exata K/N) está \
         dentro de uma priori estreita: |log10 FB| ≤ log10 √(1 + Var_priori/Var_amostral), \
         atingido quando a frequência é exatamente K/N, com Var da proporção igual a (s·K/N)² na \
         priori e p(1−p)/n na amostra. O máximo de cada janela e priori vem abaixo; a favor de \
         viés não há teto."
            .to_string(),
    );
    linha(format!(
        "AVISO: com {} dezenas, algumas saem com grau moderado ou maior só por acaso. Compare a \
         contagem observada com a esperada sob o acaso (simulação de {} réplicas, semente {}, \
         sorteio sem reposição); só um excesso sobre o esperado indica algo além do acaso.",
        r.dezenas_possiveis,
        avaliacao::REPLICAS_ESPERADO,
        avaliacao::SEMENTE_ESPERADO
    ));

    for (indice, janela) in r.janelas.iter().enumerate() {
        let ev = &janela.evidencia;
        linha(String::new());
        let nome = match ev.janela {
            avaliacao::Janela::Inteira => "histórico inteiro (principal)".to_string(),
            avaliacao::Janela::Ultimos(j) => format!("últimos {j} concursos (secundária)"),
        };
        linha(format!(
            "== Janela {}: {nome} — concursos {} ({} concursos) ==",
            indice + 1,
            descrever_faixa(ev.primeiro_concurso, ev.ultimo_concurso),
            ev.concursos
        ));
        linha(format!(
            "  {:>6}  {:>6}  {:>6}  {:>8}  {:>12}  {:<11}  {:<16}  {:>8}  {:>8}",
            "dezena",
            "x",
            "n",
            "freq (%)",
            format!("log10 FB {}", formatar_desvio(ev.prioris[0].desvio)),
            "grau",
            "direção",
            formatar_desvio(ev.prioris[1].desvio),
            formatar_desvio(ev.prioris[2].desvio),
        ));
        for d in &ev.dezenas {
            let c = d.classificacao[0];
            linha(format!(
                "  {:>6}  {:>6}  {:>6}  {:>8.2}  {:>+12.3}  {:<11}  {:<16}  {:>+8.3}  {:>+8.3}",
                format!("{:02}", d.dezena),
                d.saidas,
                d.concursos,
                100.0 * d.saidas as f64 / d.concursos as f64,
                d.log10_fb[0],
                c.grau.nome(),
                c.direcao.nome(),
                d.log10_fb[1],
                d.log10_fb[2],
            ));
        }
        linha(format!(
            "  (as duas últimas colunas são o log10 FB com a priori de {} e de {}; frequência \
             esperada sob H0 = {:.2} %.)",
            formatar_desvio(ev.prioris[1].desvio),
            formatar_desvio(ev.prioris[2].desvio),
            100.0 * ev.p
        ));

        linha(String::new());
        linha(format!(
            "  Dezenas por grau: observadas / esperadas sob o acaso (média ± erro padrão de {} \
             réplicas com n = {}):",
            janela.esperado.replicas, janela.esperado.concursos
        ));
        for (k, esperado) in janela.esperado.por_priori.iter().enumerate() {
            let rotulo = if k == 0 { " (principal)" } else { "" };
            linha(format!(
                "  Priori {}{rotulo}:",
                formatar_desvio(esperado.desvio)
            ));
            linha(format!(
                "    {:<12}  {:>24}  {:>24}",
                "grau", "a favor de viés", "a favor do acaso"
            ));
            for grau in avaliacao::Grau::TODOS {
                let celula = |direcao| {
                    format!(
                        "{:>3} / {:>7.2} ± {:.2}",
                        ev.observada[k].do_grau(grau, direcao),
                        esperado.media.do_grau(grau, direcao),
                        esperado.erro_padrao.do_grau(grau, direcao)
                    )
                };
                linha(format!(
                    "    {:<12}  {:>24}  {:>24}",
                    grau.nome(),
                    celula(avaliacao::Direcao::Vies),
                    celula(avaliacao::Direcao::Acaso)
                ));
            }
            linha(format!(
                "    Moderada ou maior (as duas direções): {} observadas contra {:.2} esperadas \
                 sob o acaso.",
                ev.observada[k].a_partir_de(avaliacao::Grau::Moderada),
                esperado.media.a_partir_de(avaliacao::Grau::Moderada)
            ));
            let teto = janela.teto_acaso[k];
            let grau_do_teto = avaliacao::classificar(teto).map_or("?", |c| c.grau.nome());
            linha(format!(
                "    Máximo atingível a favor do acaso com n = {}: |log10 FB| ≤ {teto:.3} (grau \
                 {grau_do_teto}); a favor de viés não há teto.",
                ev.concursos
            ));
        }
    }
    s
}

fn descrever_janela(janela: u32) -> String {
    if janela == rust_spider::motor::JANELA_TOTAL {
        "total".to_string()
    } else {
        janela.to_string()
    }
}

/// Momento atual em ISO-8601 UTC com segundos, para `perfis.criado_em`.
fn agora_iso8601_utc() -> rust_spider::Result<String> {
    Ok(tempo::iso8601_utc(agora_segundos()?))
}

/// Segundos desde 1970-01-01T00:00:00Z: o único ponto que lê o relógio.
fn agora_segundos() -> rust_spider::Result<u64> {
    let desde_1970 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| SpiderError::RelogioInvalido)?;
    Ok(desde_1970.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slugs(alvos: &Alvos) -> Vec<&'static str> {
        alvos.itens.iter().map(|(cfg, _)| cfg.slug).collect()
    }

    fn quina() -> &'static config::LoteriaConfig {
        config::buscar("quina").expect("quina configurada")
    }

    fn megasena() -> &'static config::LoteriaConfig {
        config::buscar("megasena").expect("megasena configurada")
    }

    #[test]
    fn resolver_alvos_recusa_db_com_todas() {
        let erro = resolver_alvos("todas", Some(PathBuf::from("x.db")), None)
            .err()
            .expect("--db com todas é recusado");
        assert!(erro.to_string().contains("--db"), "{erro}");
    }

    #[test]
    fn resolver_alvos_recusa_seletor_com_todas() {
        let erro = resolver_alvos("TODAS", None, Some("#botao"))
            .err()
            .expect("--seletor com todas é recusado");
        assert!(erro.to_string().contains("--seletor"), "{erro}");
    }

    #[test]
    fn resolver_alvos_slug_unico_usa_db_e_aceita_seletor() {
        let caminho = PathBuf::from("outro/quina.db");
        let alvos = resolver_alvos("quina", Some(caminho.clone()), Some("#botao"))
            .expect("slug único aceita --db e --seletor");
        assert!(!alvos.multiplas);
        assert_eq!(slugs(&alvos), ["quina"]);
        assert_eq!(alvos.itens[0].1, caminho);
    }

    #[test]
    fn resolver_alvos_slug_unico_sem_db_usa_padrao() {
        let alvos = resolver_alvos("MegaSena", None, None).expect("slug conhecido");
        assert!(!alvos.multiplas);
        assert_eq!(slugs(&alvos), ["megasena"]);
        assert_eq!(alvos.itens[0].1, PathBuf::from("resultados_megasena.db"));
    }

    #[test]
    fn resolver_alvos_todas_devolve_caminhos_padrao_na_ordem() {
        let alvos = resolver_alvos("todas", None, None).expect("todas é aceito");
        assert!(alvos.multiplas);
        let esperado: Vec<&str> = config::LOTERIAS.iter().map(|l| l.slug).collect();
        assert_eq!(slugs(&alvos), esperado);
        let caminhos: Vec<PathBuf> = alvos.itens.iter().map(|(_, p)| p.clone()).collect();
        assert_eq!(
            caminhos,
            [
                PathBuf::from("resultados_lotofacil.db"),
                PathBuf::from("resultados_megasena.db"),
                PathBuf::from("resultados_quina.db"),
            ]
        );
    }

    #[test]
    fn resolver_alvos_desconhecida_da_erro() {
        let erro = resolver_alvos("timemania", None, None)
            .err()
            .expect("slug desconhecido");
        assert!(erro.to_string().contains("loteria desconhecida"), "{erro}");
    }

    #[test]
    fn execucao_slug_unico_devolve_o_erro_em_registrar() {
        let mut execucao = Execucao::nova(false);
        assert!(execucao.registrar(quina(), Ok(())).is_ok());
        let erro = execucao
            .registrar(quina(), Err(anyhow::anyhow!("falha original")))
            .expect_err("slug único propaga o erro");
        assert_eq!(erro.to_string(), "falha original");
        assert!(execucao.concluir().is_ok());
    }

    #[test]
    fn execucao_todas_segue_adiante_e_concluir_lista_quem_falhou() {
        let mut execucao = Execucao::nova(true);
        assert!(execucao
            .registrar(megasena(), Err(anyhow::anyhow!("sem rede")))
            .is_ok());
        assert!(execucao.registrar(quina(), Ok(())).is_ok());
        let erro = execucao
            .concluir()
            .expect_err("alguma falhou: concluir dá erro");
        let texto = erro.to_string();
        assert!(texto.contains("1 de 2"), "{texto}");
        assert!(texto.contains("megasena"), "{texto}");
        assert!(!texto.contains("quina"), "{texto}");
    }

    #[test]
    fn execucao_todas_sem_falhas_conclui_ok() {
        let mut execucao = Execucao::nova(true);
        for cfg in config::LOTERIAS {
            assert!(execucao.registrar(cfg, Ok(())).is_ok());
        }
        assert!(execucao.concluir().is_ok());
    }

    #[test]
    fn buscar_config_unica_recusa_todas() {
        let erro = buscar_config_unica("todas", "importar", "o arquivo é de uma loteria só")
            .expect_err("todas é recusado");
        let texto = erro.to_string();
        assert!(texto.contains("importar"), "{texto}");
        assert!(texto.contains("o arquivo é de uma loteria só"), "{texto}");
        assert_eq!(
            buscar_config_unica("quina", "importar", "-")
                .expect("slug único")
                .slug,
            "quina"
        );
    }

    #[test]
    fn listar_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_listar_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = listar(quina(), &caminho).expect_err("banco inexistente dá erro");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "listar não deve criar {}", caminho.display());
    }

    #[test]
    fn inserir_com_data_no_futuro_nao_toca_no_cache_nem_no_banco() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let raiz = std::env::temp_dir().join(format!(
            "rust_spider_inserir_futuro_{}_{unico}",
            std::process::id()
        ));
        let db_caminho = raiz.join("resultados_quina.db");
        let cache = raiz.join("cache");
        std::fs::create_dir_all(&raiz).unwrap();
        let entrada = manuais::EntradaManual {
            concurso: 1,
            data_sorteio: "2099-01-01".to_string(),
            dezenas: vec![1, 2, 3, 4, 5],
        };

        let erro = inserir_manual(quina(), entrada, &db_caminho, &cache)
            .expect_err("data no futuro deve recusar");
        let sobrou = db_caminho.exists() || manuais::caminho(&cache, "quina").exists();
        let _ = std::fs::remove_dir_all(&raiz);

        assert!(erro.to_string().contains("recusado"), "{erro}");
        assert!(!sobrou, "nada pode ser criado (banco ou cache)");
    }

    #[test]
    fn features_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_features_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = calcular_features(quina(), &caminho, false).expect_err("banco inexistente");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "features não deve criar {}", caminho.display());
    }

    #[test]
    fn evidencia_recusa_todas() {
        let erro = buscar_config_unica("todas", "evidencia", MOTIVO_EVIDENCIA_UMA_LOTERIA)
            .expect_err("todas é recusado");
        let texto = erro.to_string();
        assert!(texto.contains("evidencia"), "{texto}");
        assert!(texto.contains(MOTIVO_EVIDENCIA_UMA_LOTERIA), "{texto}");
    }

    #[test]
    fn evidencia_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_evidencia_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);
        let erro = evidenciar(quina(), &caminho, std::path::Path::new("perfis"), None)
            .expect_err("banco inexistente");
        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!caminho.exists());
    }

    #[test]
    fn indice_banco_inexistente_da_erro_sem_criar_arquivo() {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_indice_{}_{unico}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);

        let erro = calcular_indice(
            quina(),
            &caminho,
            std::path::Path::new("perfis"),
            "padrao_v1",
        )
        .expect_err("banco inexistente");
        let existe = caminho.exists();
        let _ = std::fs::remove_file(&caminho);

        assert!(erro.to_string().contains("banco não encontrado"), "{erro}");
        assert!(!existe, "indice não deve criar {}", caminho.display());
    }

    #[test]
    fn indice_recusa_todas() {
        let erro = buscar_config_unica("todas", "indice", MOTIVO_INDICE_UMA_LOTERIA)
            .expect_err("todas é recusado");
        let texto = erro.to_string();
        assert!(texto.contains("`indice` não aceita `todas`"), "{texto}");
    }

    #[test]
    fn indice_exige_perfil_e_usa_a_pasta_perfis_por_padrao() {
        assert!(Cli::try_parse_from(["sync", "indice", "quina"]).is_err());
        let cli = Cli::try_parse_from(["sync", "indice", "quina", "--perfil", "padrao_v1"])
            .expect("com --perfil");
        match cli.comando {
            Comando::Indice {
                loteria,
                perfil,
                db,
                perfis,
            } => {
                assert_eq!(loteria, "quina");
                assert_eq!(perfil, "padrao_v1");
                assert_eq!(db, None);
                assert_eq!(perfis, PathBuf::from("perfis"));
            }
            _ => panic!("esperava o comando indice"),
        }
    }

    #[test]
    fn dataset_usa_parquet_sem_trancado_em_datasets_por_padrao() {
        let cli = Cli::try_parse_from(["sync", "dataset", "quina"]).expect("só a loteria");
        match cli.comando {
            Comando::Dataset {
                loteria,
                formato,
                trancado,
                saida,
                db,
            } => {
                assert_eq!(loteria, "quina");
                assert_eq!(formato, FormatoDataset::Parquet);
                assert!(!trancado);
                assert_eq!(saida, PathBuf::from("datasets"));
                assert_eq!(db, None);
            }
            _ => panic!("esperava o comando dataset"),
        }
        let cli = Cli::try_parse_from(["sync", "dataset", "quina", "--formato", "csv"])
            .expect("com --formato csv");
        assert!(matches!(
            cli.comando,
            Comando::Dataset {
                formato: FormatoDataset::Csv,
                ..
            }
        ));
    }

    #[test]
    fn dataset_recusa_todas() {
        let erro = buscar_config_unica("todas", "dataset", MOTIVO_DATASET_UMA_LOTERIA)
            .expect_err("todas é recusado");
        assert!(
            erro.to_string().contains("`dataset` não aceita `todas`"),
            "{erro}"
        );
    }

    #[test]
    fn agora_e_iso8601_utc_com_segundos() {
        let agora = agora_iso8601_utc().expect("relógio");
        assert_eq!(agora.len(), "2026-10-04T18:30:00Z".len(), "{agora}");
        assert!(agora.ends_with('Z'), "{agora}");
        assert_eq!(agora.as_bytes()[10], b'T', "{agora}");
        assert!(agora.as_str() > "2026-01-01T00:00:00Z", "{agora}");
    }

    #[test]
    fn features_recusa_todas() {
        let erro = buscar_config_unica(
            "todas",
            "features",
            "calcule uma loteria por vez (cada uma tem seu banco)",
        )
        .expect_err("todas é recusado");
        assert!(erro.to_string().contains("features"), "{erro}");
    }

    #[test]
    fn atualizar_aceita_dir_e_backups_com_padroes() {
        let cli = Cli::try_parse_from(["sync", "atualizar", "todas"]).expect("sem opções");
        match cli.comando {
            Comando::Atualizar { dir, backups, .. } => {
                assert_eq!(dir, PathBuf::from("downloads"));
                assert_eq!(backups, PathBuf::from("backups"));
            }
            _ => panic!("esperava o comando atualizar"),
        }
        let cli = Cli::try_parse_from([
            "sync",
            "atualizar",
            "quina",
            "--dir",
            "x/planilhas",
            "--backups",
            "y/copias",
        ])
        .expect("com --dir e --backups");
        match cli.comando {
            Comando::Atualizar { dir, backups, .. } => {
                assert_eq!(dir, PathBuf::from("x/planilhas"));
                assert_eq!(backups, PathBuf::from("y/copias"));
            }
            _ => panic!("esperava o comando atualizar"),
        }
    }

    fn trava_de(args: &[&str]) -> Option<&'static str> {
        let mut todos = vec!["sync"];
        todos.extend_from_slice(args);
        nome_se_grava(
            &Cli::try_parse_from(todos)
                .expect("argumentos válidos")
                .comando,
        )
    }

    #[test]
    fn so_os_comandos_que_gravam_tomam_a_trava() {
        let grava: [(&[&str], &str); 11] = [
            (&["baixar", "quina"], "baixar"),
            (&["importar", "quina", "x.xlsx"], "importar"),
            (&["atualizar", "quina"], "atualizar"),
            (
                &[
                    "inserir",
                    "quina",
                    "1",
                    "--data",
                    "2026-01-01",
                    "--dezenas",
                    "1,2,3,4,5",
                ],
                "inserir",
            ),
            (&["features", "quina"], "features"),
            (&["indice", "quina", "--perfil", "p"], "indice"),
            (&["exportar", "todas"], "exportar"),
            (&["publicar"], "publicar"),
            (&["ciclo", "todas"], "ciclo"),
            (&["dataset", "quina"], "dataset"),
            (
                &["avaliar", "quina", "--perfil", "p", "--trancado"],
                "avaliar --trancado",
            ),
        ];
        for (args, nome) in grava {
            assert_eq!(trava_de(args), Some(nome), "{args:?}");
        }
        let livres: [&[&str]; 8] = [
            &["listar", "quina"],
            &["validar", "quina"],
            &["buracos", "quina"],
            &["calendario", "quina"],
            &["aleatoriedade", "quina"],
            &["evidencia", "quina"],
            &["tabela", "quina", "--perfil", "p"],
            &["avaliar", "quina", "--perfil", "p"],
        ];
        for args in livres {
            assert_eq!(trava_de(args), None, "{args:?}");
        }
    }

    #[test]
    fn ranking_grava_toma_a_trava_e_tem_padroes() {
        assert_eq!(trava_de(&["ranking", "quina"]), Some("ranking"));
        let cli = Cli::try_parse_from(["sync", "ranking", "quina"]).expect("sem opções");
        match cli.comando {
            Comando::Ranking {
                loteria,
                perfil,
                saida,
                db,
                perfis,
                ..
            } => {
                assert_eq!(loteria, "quina");
                // Sem `--perfil`, vale o perfil ativo da loteria (decidido na execução).
                assert_eq!(perfil, None);
                assert_eq!(saida, PathBuf::from("ranking"));
                assert_eq!(db, None);
                assert_eq!(perfis, PathBuf::from("perfis"));
            }
            _ => panic!("esperava o comando ranking"),
        }
        let cli = Cli::try_parse_from([
            "sync", "ranking", "megasena", "--perfil", "busca_v1", "--saida", "x/r", "--db",
            "y.db", "--perfis", "z",
        ])
        .expect("com todas as opções");
        match cli.comando {
            Comando::Ranking {
                perfil,
                saida,
                db,
                perfis,
                ..
            } => {
                assert_eq!(perfil.as_deref(), Some("busca_v1"));
                assert_eq!(saida, PathBuf::from("x/r"));
                assert_eq!(db, Some(PathBuf::from("y.db")));
                assert_eq!(perfis, PathBuf::from("z"));
            }
            _ => panic!("esperava o comando ranking"),
        }
    }

    #[test]
    fn ranking_aceita_avaliar_sem_gravar_e_recusa_saida() {
        // `--avaliar` só lê o banco (a trava do comando continua a de `ranking`).
        assert_eq!(
            trava_de(&["ranking", "quina", "--avaliar"]),
            Some("ranking")
        );
        let cli = Cli::try_parse_from(["sync", "ranking", "quina"]).expect("sem opções");
        assert!(matches!(
            cli.comando,
            Comando::Ranking { avaliar: false, .. }
        ));
        let cli = Cli::try_parse_from([
            "sync",
            "ranking",
            "megasena",
            "--avaliar",
            "--perfil",
            "busca_v1",
            "--db",
            "y.db",
            "--perfis",
            "z",
        ])
        .expect("com --avaliar");
        match cli.comando {
            Comando::Ranking {
                avaliar,
                perfil,
                db,
                perfis,
                ..
            } => {
                assert!(avaliar);
                assert_eq!(perfil.as_deref(), Some("busca_v1"));
                assert_eq!(db, Some(PathBuf::from("y.db")));
                assert_eq!(perfis, PathBuf::from("z"));
            }
            _ => panic!("esperava o comando ranking"),
        }
        // Nada é gravado: `--saida` junto de `--avaliar` não faz sentido.
        assert!(
            Cli::try_parse_from(["sync", "ranking", "quina", "--avaliar", "--saida", "x"]).is_err()
        );
    }

    #[test]
    fn ranking_recusa_todas() {
        let erro = buscar_config_unica("todas", "ranking", MOTIVO_RANKING_UMA_LOTERIA)
            .expect_err("`todas` é recusada");
        let texto = erro.to_string();
        assert!(texto.contains("`ranking` não aceita `todas`"), "{texto}");
        assert!(
            buscar_config_unica("quina", "ranking", MOTIVO_RANKING_UMA_LOTERIA).is_ok(),
            "um slug passa"
        );
    }
}
