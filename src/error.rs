use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpiderError {
    #[error("loteria desconhecida: `{0}`")]
    LoteriaDesconhecida(String),

    #[error("erro do navegador ao acessar `{url}`: {detalhe}")]
    Navegador { url: String, detalhe: String },

    #[error("elemento de download não encontrado (seletor `{0}`) — a página pode ter mudado de layout ou bloqueado o acesso")]
    BotaoDownloadNaoEncontrado(String),

    #[error("nenhum download foi iniciado após clicar no seletor `{0}`")]
    DownloadNaoIniciado(String),

    #[error("elemento não encontrado (seletor `{0}`) — a página pode ter mudado de layout ou bloqueado o acesso")]
    ElementoNaoEncontrado(String),

    #[error("erro de E/S em `{caminho}`")]
    Io {
        caminho: PathBuf,
        #[source]
        origem: std::io::Error,
    },

    #[error("erro ao ler a planilha `{caminho}`: {detalhe}")]
    Planilha { caminho: PathBuf, detalhe: String },

    #[error("tabela de resultados não encontrada ou em layout inesperado em `{0}`")]
    TabelaNaoEncontrada(PathBuf),

    #[error("dado inválido: {0}")]
    LinhaInvalida(String),

    #[error("cache de inserções manuais `{caminho}` inválido: {motivo}")]
    CacheManualInvalido { caminho: PathBuf, motivo: String },

    /// O motor conta as janelas pelo número do concurso, então não aceita
    /// buraco na numeração (ver "Convenções" em `docs/PLANEJAMENTO.md`).
    #[error("motor: faltam os concursos {esperado}..={} antes do {recebido} — feche o buraco (`sync buracos`/`sync inserir`) antes de calcular as features", recebido.saturating_sub(1))]
    MotorBuraco { esperado: u32, recebido: u32 },

    #[error("motor: concurso {0} recebido de novo — cada concurso entra uma única vez")]
    MotorConcursoRepetido(u32),

    #[error("motor: concurso {recebido} fora de ordem (o último incorporado foi o {ultimo}) — os concursos devem vir em ordem crescente")]
    MotorForaDeOrdem { ultimo: u32, recebido: u32 },

    #[error("motor: concurso {concurso} inválido: {detalhe} — rode `sync validar`")]
    MotorResultadoInvalido { concurso: u32, detalhe: String },

    #[error(
        "motor: estado incompatível com a configuração atual: {0} — regenere as features do zero"
    )]
    MotorEstadoIncompativel(String),

    /// O índice normaliza entre as dezenas de cada concurso e mede a variação
    /// contra o concurso anterior: precisa de uma série completa de features
    /// (concursos seguidos, as mesmas dezenas em todos, a janela do perfil) e
    /// de pesos finitos. O detalhe já traz o que fazer (regenerar as features
    /// ou corrigir o perfil).
    #[error("motor: entrada inválida para o índice de força: {0}")]
    MotorIndiceEntradaInvalida(String),

    /// `sync features` só roda sobre um histórico que passa no `validar`
    /// (buraco incluso). Os problemas vão junto para a CLI imprimir.
    #[error("{loteria}: {} erro(s) de integridade nos resultados — nada foi calculado nem gravado; confira com `sync validar` e feche buracos com `sync buracos`/`sync inserir`", erros.len())]
    ResultadosReprovados {
        loteria: String,
        erros: Vec<crate::validacao::Problema>,
        avisos: Vec<crate::validacao::Problema>,
    },

    /// Perfil de pesos que não passa na validação rígida (`docs/PLANEJAMENTO.md`,
    /// "Perfil de pesos"). `motivo` cita o campo.
    #[error("perfil `{}` inválido: {motivo}", origem.display())]
    PerfilInvalido { origem: PathBuf, motivo: String },

    /// Perfis são imutáveis: o mesmo nome com outro conteúdo deixaria índices
    /// já gravados com um perfil diferente do que os gerou.
    #[error("perfil `{nome}` já registrado neste banco com outro conteúdo (hash registrado {hash_registrado}, hash do arquivo {hash_arquivo}) — perfis são imutáveis: desfaça a edição ou crie `{proximo_nome}.toml` com `nome = \"{proximo_nome}\"` e `versao = {proxima_versao}`")]
    PerfilAlterado {
        nome: String,
        hash_registrado: String,
        hash_arquivo: String,
        proximo_nome: String,
        proxima_versao: u32,
    },

    /// Um banco por loteria: perfil de outra loteria gravaria na tabela
    /// `perfis` (e depois no índice) pesos pensados para outro jogo.
    #[error("perfil `{nome}` é da loteria `{loteria_perfil}`, mas o banco é da `{loteria_banco}` — use o perfil em `perfis/{loteria_banco}/`")]
    PerfilDeOutraLoteria {
        nome: String,
        loteria_perfil: String,
        loteria_banco: String,
    },

    #[error("nenhum concurso de `{0}` no banco — rode `sync baixar` ou `sync importar` antes")]
    SemResultados(String),

    /// `sync indice` lê o índice de `features_dezena`; sem features não há o
    /// que indexar (e nada é criado no banco).
    #[error("nenhuma feature de `{0}` no banco (features_dezena vazia ou ausente) — rode `sync features {0}` antes; nada foi gravado")]
    SemFeatures(String),

    /// O índice tem de cobrir até o snapshot do próximo concurso (último de
    /// `resultados` + 1), que é o da tabela geral do app; features de outro
    /// ponto da série dariam um índice velho (ou de concursos que não existem
    /// mais).
    #[error("{loteria}: as features vão até o snapshot do concurso {snapshot}, mas o último concurso em `resultados` é o {ultimo_concurso} (esperado o snapshot do {}) — rode `sync features {loteria}` antes; nada foi gravado", ultimo_concurso + 1)]
    FeaturesDesatualizadas {
        loteria: String,
        ultimo_concurso: u32,
        snapshot: u32,
    },

    /// Algum resultado já incorporado às features foi corrigido (ou o estado
    /// do motor sumiu) depois do último `sync features`: o último concurso é
    /// o mesmo, mas as features foram calculadas com outro histórico.
    #[error("{loteria}: os resultados mudaram desde o último `sync features` (impressão de `resultados` diferente da de `motor_estado`) — rode `sync features {loteria}` antes; nada foi gravado")]
    FeaturesDeResultadosAlterados { loteria: String },

    /// Features calculadas com outra versão das fórmulas não servem para o
    /// índice: o perfil foi pensado para as fórmulas atuais.
    #[error("{loteria}: features_dezena tem linhas da versão {encontrada} das fórmulas, o código está na versão {atual} — rode `sync features {loteria}` antes; nada foi gravado")]
    FeaturesDeOutraVersao {
        loteria: String,
        encontrada: u32,
        atual: u32,
    },

    /// `features_dezena` com linhas faltando ou sobrando na janela do perfil
    /// (dezena fora do universo da loteria, concurso sem todas as dezenas,
    /// série que não começa no 1). Só acontece se a tabela foi mexida por
    /// fora; regenerar do zero resolve.
    #[error("{loteria}: features_dezena incoerente na janela {janela}: {detalhe} — regenere com `sync features {loteria} --completo`; nada foi gravado")]
    FeaturesIncoerentes {
        loteria: String,
        janela: u32,
        detalhe: String,
    },

    /// A calibração índice → probabilidade percorre a série concurso a
    /// concurso: precisa de concursos seguidos, todas as dezenas da loteria
    /// com posições 1…N em cada um, e *K* saídas por concurso sorteado (só o
    /// último, o snapshot do próximo sorteio, pode estar sem resultado).
    #[error("avaliação: entrada inválida: {0} — confira a montagem das observações ou regenere as features e o índice (`sync features`/`sync indice`)")]
    AvaliacaoEntradaInvalida(String),

    /// Nenhum concurso no trecho pedido (ou menos do que o necessário, como
    /// nas dobras do walk-forward). O trecho depende de quem chama — busca até
    /// o corte, trancado acima dele, faixa explícita, série já calculada —,
    /// então o detalhe é montado em cada ponto de chamada e diz qual trecho
    /// ficou vazio e por quê.
    #[error("avaliação: nada para avaliar: {0}")]
    AvaliacaoSemConcursos(String),

    /// Os testes de aleatoriedade (1c.4) leem `resultados` direto, sem passar
    /// por features nem índice: concurso repetido, buraco na numeração ou
    /// concurso que não passa nas regras de `validacao` (outra loteria,
    /// quantidade de dezenas, faixa, repetidas, data) é problema do histórico
    /// gravado, não da montagem da avaliação.
    #[error("aleatoriedade: resultados inválidos para os testes: {0} — confira com `sync validar` e feche buracos com `sync buracos`/`sync inserir`; nada foi testado")]
    AleatoriedadeEntradaInvalida(String),

    /// A evidência bayesiana por dezena (1d.2) lê `resultados` direto: os
    /// concursos usados precisam ser seguidos e válidos; os parâmetros
    /// (chance, priori, `x ≤ n`) precisam fazer sentido. O detalhe diz qual.
    #[error("evidência: entrada inválida: {0} — confira com `sync validar` e feche buracos com `sync buracos`/`sync inserir`; nada foi calculado")]
    EvidenciaEntradaInvalida(String),

    /// O `sync avaliar` lê o índice gravado pelo `sync indice`, e o perfil
    /// tem de estar em `perfis` (registrado com o hash do `.toml`).
    #[error("perfil `{nome}` não está registrado em `perfis` no banco da {loteria} — rode `sync indice {loteria} --perfil {nome}` antes; nada foi avaliado")]
    PerfilNaoRegistrado { nome: String, loteria: String },

    /// O índice do perfil não cobre do primeiro concurso ao snapshot do
    /// próximo (último de `resultados` + 1): foi gravado antes de concursos
    /// novos ou nunca foi gravado.
    #[error("{loteria}: o índice do perfil `{perfil}` vai até o concurso {}, mas tem de ir até o {esperado} (último concurso em `resultados` + 1) — rode `sync indice {loteria} --perfil {perfil}` antes; nada foi avaliado", ultimo_indice.map_or_else(|| "nenhum".to_string(), |c| c.to_string()))]
    IndiceDesatualizado {
        loteria: String,
        perfil: String,
        ultimo_indice: Option<u32>,
        esperado: u32,
    },

    /// Índice com buraco, dezena faltando ou fora das features (só acontece
    /// se a tabela foi mexida por fora: o `sync indice` grava a série
    /// inteira numa transação), ou diferente do índice recalculado com as
    /// features atuais: features regeneradas depois do último `sync indice`,
    /// como após corrigir um resultado antigo.
    #[error("{loteria}: índice do perfil `{perfil}` incoerente: {detalhe} — regenere com `sync indice {loteria} --perfil {perfil}`; nada foi avaliado")]
    IndiceIncoerente {
        loteria: String,
        perfil: String,
        detalhe: String,
    },

    /// `sync tabela --concurso N` fora dos concursos com índice.
    #[error("{loteria}: não há tabela do concurso {concurso} — escolha um concurso de 1 a {maximo} (o {maximo} é o próximo, ainda não sorteado)")]
    TabelaConcursoInvalido {
        loteria: String,
        concurso: u32,
        maximo: u32,
    },

    /// Combinação de opções do `sync avaliar` que não faz sentido.
    #[error("avaliação: {0}")]
    AvaliacaoOpcaoInvalida(String),

    /// O arquivo de registro do trancado não pôde ser interpretado: na
    /// dúvida, o trancado não é avaliado.
    #[error("registro do conjunto trancado `{}` inválido: {motivo} — corrija o arquivo (está no git); nada foi avaliado", caminho.display())]
    RegistroTrancadoInvalido { caminho: PathBuf, motivo: String },

    /// O registro do trancado mudou entre a recarga e a gravação em todas as
    /// tentativas: outra execução com `--trancado` está gravando ao mesmo
    /// tempo. Nada é registrado nem mostrado.
    #[error("o registro do conjunto trancado `{}` mudou durante a avaliação {tentativas} vez(es) seguidas (outra execução com `--trancado` em paralelo?) — nada foi registrado nem mostrado; rode de novo, uma execução por vez", caminho.display())]
    RegistroTrancadoConcorrente { caminho: PathBuf, tentativas: u32 },

    /// O conjunto trancado é de uso único por perfil (mesmo nome e hash).
    /// `registro` diz qual arquivo foi lido (caminho absoluto) e quantos
    /// usos, de qualquer perfil, ele tinha (em caixa: mantém o erro pequeno).
    #[error("{loteria}: o perfil `{perfil}` (hash {hash_toml}) já avaliou o conjunto trancado {} vez(es), a última em {} (até o concurso {}), segundo o registro {} ({} uso(s) no total) — o trancado é de uso único por perfil; nada foi avaliado. Para olhar de novo de propósito (a nova avaliação também fica registrada), use `--repetir`", anteriores.len(), anteriores.last().map_or("?", |a| a.avaliado_em.as_str()), anteriores.last().map_or(0, |a| a.ate_concurso), registro.caminho.display(), registro.usos)]
    TrancadoJaAvaliado {
        loteria: String,
        perfil: String,
        hash_toml: String,
        anteriores: Vec<crate::registros::AvaliacaoTrancado>,
        registro: Box<crate::registros::RegistroConsultado>,
    },

    /// Cópia de segurança do banco antes de um `sync atualizar` com salto.
    #[error("backup do banco em `{}`: {motivo}", caminho.display())]
    Backup { caminho: PathBuf, motivo: String },

    /// Comandos que só leem recusam banco inexistente (`db::abrir` criaria
    /// um arquivo vazio).
    #[error("banco não encontrado: {}", .0.display())]
    BancoNaoEncontrado(PathBuf),

    /// O relógio do sistema não serve para carimbar backups e registros.
    #[error("relógio do sistema antes de 1970-01-01; acerte a data")]
    RelogioInvalido,

    /// Lote recusado pela validação de gravação (tudo ou nada). Os erros vão
    /// junto para quem quiser listá-los; nada foi gravado.
    #[error("{loteria}: lote recusado por {} erro(s) de integridade; nada foi gravado em {}", erros.len(), caminho.display())]
    LoteRecusado {
        loteria: String,
        erros: Vec<crate::validacao::Problema>,
        caminho: PathBuf,
    },

    /// O `sync atualizar` com salto segue o caminho da planilha completa; se
    /// ele falha, o banco ficou como estava e o backup (se houve) aponta o
    /// estado de antes da tentativa. A causa vem em `origem`.
    #[error("{loteria}: o caminho da planilha completa falhou{}", backup.as_ref().map_or_else(String::new, |c| format!("; o backup do banco de antes da tentativa está em {}", c.display())))]
    CaminhoDaPlanilhaFalhou {
        loteria: String,
        backup: Option<PathBuf>,
        #[source]
        origem: Box<SpiderError>,
    },

    /// Dados do `sync dataset` que não respeitam o que a montagem dos
    /// arquivos pressupõe (ordem, só concursos sorteados, trancado só com
    /// `--trancado`). Só acontece por erro de programação ou tabela mexida por
    /// fora: as features já foram conferidas antes.
    #[error("dataset incoerente: {0} — regenere com `sync features --completo`; nada foi gravado")]
    DatasetIncoerente(String),

    /// Valor do dataset acima de `i32::MAX`, o limite das colunas INT32 do
    /// Parquet. Regenerar as features não resolve: o valor é legítimo e o
    /// esquema é que não comporta.
    #[error("dataset: o valor {0} não cabe numa coluna INT32 do Parquet (máximo 2147483647) — o esquema do Parquet precisa de colunas maiores (`ESQUEMA_PARQUET` em src/dataset.rs); enquanto isso, use `--formato csv`; nada foi gravado")]
    ValorForaDoInt32(u32),

    /// Falha do serde_json ao serializar o manifesto do `sync dataset`. Só
    /// acontece por erro de programação na struct do manifesto.
    #[error("dataset: falha ao serializar o manifesto em JSON: {0} — erro de programação em `Manifesto` (src/dataset.rs), não nos dados; nada foi gravado")]
    ManifestoJson(#[source] serde_json::Error),

    /// Saída do `sync exportar` que não pode ser aproveitada ou dado que não
    /// cabe no contrato `v1`. Nada foi gravado.
    #[error("exportação: {0}; nada foi gravado")]
    ExportacaoInvalida(String),

    /// Saída do `sync exportar` que não pode ser publicada: sem `index.json`,
    /// arquivo que não bate com o `sha256`/`bytes` do índice, arquivo ausente
    /// ou fora do índice. Nada foi copiado.
    #[error("publicação: a saída `{}` não pode ser publicada: {motivo} — rode `sync exportar todas` de novo; nada foi copiado", caminho.display())]
    PublicacaoSaidaInvalida { caminho: PathBuf, motivo: String },

    /// O destino não está em condições de receber (clone ausente, sujo, fora
    /// de `main`, atrás ou divergente de `origin/main`) ou o `git` falhou antes
    /// de qualquer envio. A mensagem diz o que fazer.
    #[error("publicação: {0}")]
    PublicacaoRecusada(String),

    /// O commit existe só no clone local; a próxima execução tenta de novo.
    #[error("publicação: o envio ao remoto falhou ({0}) — o commit ficou só no clone local; confira a rede e o SSH e rode `sync publicar` de novo, que tenta o envio outra vez")]
    PublicacaoEnvioFalhou(String),

    #[error("erro ao montar o Parquet: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("erro de banco de dados: {0}")]
    Db(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, SpiderError>;
