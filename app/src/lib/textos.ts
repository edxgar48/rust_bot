// Todos os textos de interface ficam neste módulo.
import type {
  ChaveComponente,
  DirecaoEvidencia,
  GrauEvidencia,
  Normalizacao,
} from '../dados/tipos';

export const textos = {
  appNome: 'Força das Dezenas',
  navegacao: 'Navegação principal',
  abas: {
    inicio: 'Início',
    forca: 'Força',
    conferir: 'Conferir',
    estatisticas: 'Estatísticas',
    entenda: 'Entenda',
  },
  placeholder: 'Esta tela ainda está em construção.',
  sincronizacao: {
    titulo: 'Situação dos dados',
    sincronizando: (data: string | null) =>
      data === null
        ? 'Verificando atualizações…'
        : `Verificando atualizações… Mostrando dados de ${data}.`,
    atualizado: (data: string) => `Atualizado em ${data}.`,
    semRede: (data: string | null) =>
      data === null
        ? 'Sem rede, mostrando os últimos dados salvos.'
        : `Sem rede, mostrando dados de ${data}.`,
    recusado: (data: string | null) =>
      data === null
        ? 'Dado recusado (falha de conferência), mantendo os anteriores.'
        : `Dado recusado (falha de conferência), mantendo os anteriores, de ${data}.`,
    semDados: 'Ainda sem dados. Conecte-se à internet para baixar a primeira vez.',
    armazenamentoVolatil:
      'Este navegador não permite guardar dados neste aparelho (modo privado ou bloqueado). Os dados valem só até fechar o app.',
    atualizacaoDisponivel: 'Nova versão do app pronta. Ela vale na próxima vez que você abrir.',
  },
  loterias: {
    lotofacil: 'Lotofácil',
    megasena: 'Mega-Sena',
    quina: 'Quina',
  },
  inicio: {
    titulo: 'Resultados',
    concursoData: (concurso: number, data: string) => `Concurso ${concurso} · ${data}`,
    dezenasSorteadas: (loteria: string) => `Dezenas sorteadas na ${loteria}`,
    semResultado: 'Sem dezenas guardadas para este concurso.',
    semLoteria: 'Ainda sem dados desta loteria',
    carregando: 'Carregando resultados…',
    atualizar: 'Atualizar',
    atualizando: 'Atualizando…',
    tentarDeNovo: 'Tentar de novo',
  },
  forca: {
    titulo: 'Tabela de força',
    subtitulo: (proximo: number, ultimoSorteado: number) =>
      `Próximo concurso ${proximo}, com dados até o ${ultimoSorteado}`,
    perfilJanela: (perfil: string, janela: number) =>
      janela === 0
        ? `Perfil ${perfil} · histórico total`
        : `Perfil ${perfil} · janela de ${janela} ${janela === 1 ? 'concurso' : 'concursos'}`,
    rotuloLoteria: 'Loteria',
    rotuloVisao: 'Forma de exibição',
    visaoLista: 'Lista',
    visaoVolante: 'Volante',
    colunas: {
      posicao: 'Pos.',
      dezena: 'Dez.',
      variacao: 'Variação',
      indice: 'Índice',
      evidencia: 'Evidência (grau · direção)',
    },
    lista: (loteria: string, quantidade: number) =>
      `Ranking das ${quantidade} dezenas da ${loteria}`,
    carregando: 'Carregando a tabela de força…',
    tentarDeNovo: 'Tentar de novo',
    tentando: 'Tentando…',
    semLoteria: 'Ainda sem dados desta loteria.',
    recusadoSemAnterior:
      'O arquivo desta loteria foi recusado (falha de conferência) e não há versão anterior guardada.',
  },
  // Histórico de concursos (aba Conferir, 6.1).
  historico: {
    titulo: 'Histórico de concursos',
    subtitulo: (quantidade: number, primeiro: number, ultimo: number) =>
      `${quantidade.toLocaleString('pt-BR')} concursos, do ${primeiro} ao ${ultimo}`,
    rotuloLoteria: 'Loteria',
    carregando: 'Carregando o histórico…',
    tentarDeNovo: 'Tentar de novo',
    semLoteria: 'Ainda sem dados desta loteria.',
    recusadoSemAnterior:
      'O histórico desta loteria foi recusado (falha de conferência) e não há versão anterior guardada.',
    busca: {
      titulo: 'Buscar um concurso',
      rotuloModo: 'Buscar por',
      modoConcurso: 'Número',
      modoData: 'Data',
      campoConcurso: 'Número do concurso',
      campoData: 'Data do sorteio',
      exemploConcurso: 'Ex.: 3000',
      exemploData: 'dd/mm/aaaa',
      buscar: 'Buscar',
      limpar: 'Limpar busca',
      ativaConcurso: (numero: number) => `Busca: concurso ${numero}`,
      ativaData: (data: string) => `Busca: sorteios de ${data}`,
      formatoConcurso: 'Digite o número do concurso, só com algarismos. Exemplo: 3000.',
      formatoData: 'Digite a data como dia/mês/ano. Exemplo: 13/10/2026.',
      dataInexistente: (data: string) => `${data} não é uma data do calendário.`,
      concursoInexistente: (numero: number, primeiro: number, ultimo: number) =>
        `Não existe o concurso ${numero}. Esta loteria tem os concursos de ${primeiro} a ${ultimo}.`,
      dataSemSorteio: (data: string) => `Não houve sorteio em ${data}.`,
      vizinhoAnterior: (concurso: number, data: string) =>
        `Sorteio anterior mais próximo: concurso ${concurso}, em ${data}.`,
      vizinhoSeguinte: (concurso: number, data: string) =>
        `Sorteio seguinte mais próximo: concurso ${concurso}, em ${data}.`,
    },
    filtro: {
      titulo: 'Filtrar por dezenas',
      escolhidas: (quantidade: number) =>
        quantidade === 0
          ? 'nenhuma escolhida'
          : `${quantidade} ${quantidade === 1 ? 'escolhida' : 'escolhidas'}`,
      intro:
        'Toque nas dezenas para ver só os concursos em que todas elas saíram juntas. É uma consulta ao passado.',
      limite: (maximo: number) =>
        `Limite de ${maximo} dezenas: é quantas saem em cada concurso. Desmarque uma para trocar.`,
      limpar: 'Limpar dezenas',
      ativas: (dezenas: string) => `Dezenas escolhidas: ${dezenas}`,
    },
    contagem: {
      tudo: (total: number) =>
        `${total.toLocaleString('pt-BR')} ${total === 1 ? 'concurso' : 'concursos'}, do mais recente ao mais antigo`,
      encontrados: (encontrados: number, total: number) =>
        `${encontrados.toLocaleString('pt-BR')} ${encontrados === 1 ? 'concurso encontrado' : 'concursos encontrados'}, de ${total.toLocaleString('pt-BR')}, do mais recente ao mais antigo`,
      nenhumCurto: 'Nenhum concurso encontrado.',
      nenhum: (busca: boolean, filtro: boolean) =>
        busca && filtro
          ? 'Nenhum concurso corresponde à busca e às dezenas escolhidas.'
          : busca
            ? 'Nenhum concurso corresponde à busca.'
            : 'Nenhum concurso tem todas as dezenas escolhidas.',
    },
    lista: (loteria: string, resultados: number) =>
      `Concursos da ${loteria}: ${resultados.toLocaleString('pt-BR')} ${resultados === 1 ? 'resultado' : 'resultados'}`,
    concurso: (numero: number) => `Concurso ${numero}`,
    // Leitura da linha por leitor de tela (as bolas são só visuais).
    dezenasDoConcurso: (dezenas: string, filtradas: string | null) =>
      filtradas === null
        ? `Dezenas sorteadas: ${dezenas}.`
        : `Dezenas sorteadas: ${dezenas}. Dezenas do filtro neste concurso: ${filtradas}.`,
  },
  detalhe: {
    titulo: (dezena: number, posicao: number) => `Dezena ${dezena} · ${posicao}º lugar`,
    fechar: 'Fechar',
    indice: 'Índice de força',
    variacao: 'Em relação ao concurso anterior',
    evidencia: 'Evidência sobre o acaso',
    semEvidencia: 'Sem evidência calculada para esta dezena.',
    janela: (janela: number) =>
      janela === 0
        ? 'Janela dos componentes: histórico inteiro'
        : `Janela dos componentes: últimos ${janela} ${janela === 1 ? 'concurso' : 'concursos'}`,
    componentes: 'Componentes do índice',
    componentesIntro:
      'Os nove números abaixo, medidos na janela, são a base do índice. Eles descrevem o passado.',
    evidenciaNota:
      'A evidência mede o quanto os dados convencem, não o tamanho do desvio, e nunca prevê o sorteio.',
    semDados: 'sem dados na janela',
    nuncaSaiu: '-',
  },
  // Avisos permanentes das telas. Ficam juntos para a revisão de linguagem (5.4)
  // e para o teste de termos proibidos; cada tela escolhe o seu, sem texto próprio.
  avisos: {
    naoPreveInicio:
      'Cada sorteio é independente: nenhuma dezena tem mais chance de sair por causa do histórico. Este app não prevê resultados.',
    naoPreveForca: 'O índice resume o passado de cada dezena. Ele não prevê o sorteio.',
    naoPreveDetalhe:
      'O índice descreve o passado desta dezena e não prevê o sorteio. Cada sorteio é independente.',
    naoPreveHistorico:
      'O histórico mostra o que já foi sorteado. Ele não prevê o sorteio: cada concurso é independente dos anteriores.',
    naoPreveConferir:
      'Conferir o passado não aumenta a chance de acertar. Cada sorteio é independente dos anteriores.',
    // Telas de jogo (Entenda e Conferir jogo).
    jogoResponsavel: 'Jogue com responsabilidade. Apostas são para maiores de 18 anos.',
  },
  // Aba Conferir: troca entre as duas seções e tela Conferir jogo (6.2).
  conferir: {
    rotuloSecao: 'Seção',
    secaoJogo: 'Conferir jogo',
    secaoHistorico: 'Histórico',
    secaoJogos: 'Meus jogos',
    titulo: 'Conferir jogo',
    subtitulo: (loteria: string, minimo: number, maximo: number) =>
      `${loteria} · escolha de ${minimo} a ${maximo} dezenas`,
    rotuloLoteria: 'Loteria',
    carregando: 'Carregando o histórico…',
    tentarDeNovo: 'Tentar de novo',
    semLoteria: 'Ainda sem dados desta loteria.',
    recusadoSemAnterior:
      'O histórico desta loteria foi recusado (falha de conferência) e não há versão anterior guardada.',
    volanteTitulo: 'Marque as dezenas do jogo',
    marcadas: (quantidade: number) =>
      `${quantidade} ${quantidade === 1 ? 'dezena marcada' : 'dezenas marcadas'}`,
    limpar: 'Limpar',
    limite: (maximo: number) => `Limite de ${maximo} dezenas por jogo. Desmarque uma para trocar.`,
    quantidade: {
      vazia: (minimo: number, maximo: number) =>
        `Marque de ${minimo} a ${maximo} dezenas no volante para ver este jogo nos concursos passados.`,
      faltam: (faltam: number, minimo: number) =>
        `Marque mais ${faltam} ${faltam === 1 ? 'dezena' : 'dezenas'} para conferir: o jogo precisa de pelo menos ${minimo}.`,
      excede: (sobram: number, maximo: number) =>
        `Desmarque ${sobram} ${sobram === 1 ? 'dezena' : 'dezenas'}: o jogo aceita no máximo ${maximo}.`,
    },
    // Salvar o jogo marcado no volante (6.3).
    salvar: {
      rotuloNome: 'Nome do jogo (opcional)',
      ajudaNome: (maximo: number) => `Até ${maximo} caracteres.`,
      botao: 'Salvar jogo',
      salvando: 'Salvando…',
      salvo: 'Jogo salvo em Meus jogos, neste aparelho.',
      verMeusJogos: 'Ver Meus jogos',
      duplicado: (nome: string | null) =>
        nome === null
          ? 'Este jogo já está salvo, sem nome.'
          : `Este jogo já está salvo, com o nome "${nome}".`,
      invalido: 'Este jogo não pode ser salvo: confira a quantidade de dezenas e o nome.',
      falhou: 'Não foi possível salvar o jogo.',
      volatil:
        'Este navegador não está guardando jogos no aparelho: o jogo vale só até fechar o app. Use Exportar em Meus jogos para guardar uma cópia.',
      carregado: (nome: string | null) =>
        nome === null ? 'Jogo salvo carregado no volante.' : `Jogo "${nome}" carregado no volante.`,
    },
    resultado: {
      titulo: (total: number) =>
        total === 1
          ? 'Este jogo no único concurso'
          : `Este jogo em todos os ${total.toLocaleString('pt-BR')} concursos`,
      melhor: (acertos: number, concurso: number) =>
        `Melhor resultado: ${acertos} ${acertos === 1 ? 'acerto' : 'acertos'} (concurso ${concurso})`,
      melhorRepetido: (vezes: number) =>
        `Esse resultado ocorreu em ${vezes.toLocaleString('pt-BR')} concursos; o citado é o mais recente.`,
      etiquetaMelhor: 'melhor resultado',
      listaMelhor: 'Concursos com o melhor resultado',
      listaMelhorResumo: (mostrados: number, total: number) =>
        mostrados < total
          ? `Mostrando os ${mostrados} mais recentes, de ${total.toLocaleString('pt-BR')}.`
          : '',
      concursoItem: (concurso: number) => `Concurso ${concurso}`,
      semConcursos: 'Este histórico ainda não tem concursos para conferir.',
      faixas: 'Concursos por número de acertos',
      ultimo: (concurso: number, acertos: number) =>
        `No último concurso (${concurso}): ${acertos} ${acertos === 1 ? 'acerto' : 'acertos'}`,
      faixaSemConcursos: 'Nenhum concurso nesta faixa.',
      faixaMais: (restantes: number) => `e mais ${restantes.toLocaleString('pt-BR')}`,
      acertos: (acertos: number) => `${acertos} ${acertos === 1 ? 'acerto' : 'acertos'}`,
      concursos: (concursos: number) =>
        `${concursos.toLocaleString('pt-BR')} ${concursos === 1 ? 'concurso' : 'concursos'}`,
      nota: 'Mostra o que este jogo teria feito nos concursos já sorteados. É uma consulta ao passado.',
    },
  },
  // Jogos salvos no aparelho (aba Conferir, 6.3).
  jogos: {
    titulo: 'Meus jogos',
    rotuloLoteria: 'Loteria',
    rotuloBackup: 'Backup dos jogos',
    localAviso:
      'Os jogos ficam só neste aparelho e neste navegador: não há conta nem nuvem. Limpar os dados do navegador apaga os jogos. O backup é o arquivo gerado por Exportar; guarde-o onde quiser e use Importar para recuperar os jogos.',
    volatil:
      'Não foi possível abrir o armazenamento deste navegador (ele pode estar indisponível, bloqueado ou em modo privado). Os jogos de agora valem só até fechar o app e os jogos já salvos antes podem não aparecer aqui. Use Exportar para guardar uma cópia.',
    carregando: 'Carregando seus jogos…',
    falhaListar:
      'Não foi possível ler os jogos salvos agora. A lista não é exibida e Exportar fica indisponível até a leitura funcionar. Tente de novo.',
    tentarDeNovo: 'Tentar ler de novo',
    apagarFalhou: 'Não foi possível apagar o jogo. Ele continua salvo.',
    desfazerFalhou: 'Não foi possível restaurar o jogo apagado.',
    vazio: (loteria: string) => `Nenhum jogo salvo na ${loteria}.`,
    vazioOrienta: 'Marque as dezenas em Conferir jogo e toque em Salvar jogo.',
    irParaConferir: 'Ir para Conferir jogo',
    lista: (loteria: string, quantidade: number) =>
      `Jogos salvos da ${loteria}: ${quantidade} ${quantidade === 1 ? 'jogo' : 'jogos'}`,
    semNome: 'Jogo sem nome',
    resumo: (loteria: string, quantidade: number) =>
      `${loteria} · ${quantidade} ${quantidade === 1 ? 'dezena' : 'dezenas'}`,
    dezenasRotulo: 'Dezenas do jogo',
    salvoEm: (data: string) => `Salvo em ${data}`,
    melhor: (acertos: number, concurso: number) =>
      `Melhor resultado no histórico: ${acertos} ${acertos === 1 ? 'acerto' : 'acertos'} (concurso ${concurso})`,
    conferir: 'Conferir',
    renomear: 'Renomear',
    apagar: 'Apagar',
    // Nome acessível dos botões de cada jogo (o texto visível é curto).
    acaoDe: (acao: string, descricao: string) => `${acao}: ${descricao}`,
    campoNome: 'Nome do jogo',
    salvarNome: 'Salvar nome',
    cancelar: 'Cancelar',
    nomeAtualizado: 'Nome atualizado.',
    nomeInvalido: (maximo: number) => `O nome aceita até ${maximo} caracteres.`,
    renomearFalhou: 'Não foi possível trocar o nome: o jogo não está mais salvo.',
    confirmaTitulo: 'Apagar este jogo?',
    confirmaTexto: (descricao: string) =>
      `${descricao} será apagado deste aparelho. Se você não tem um arquivo exportado, não há outra cópia. Logo depois de apagar ainda dá para desfazer.`,
    confirmaApagar: 'Apagar jogo',
    apagado: 'Jogo apagado.',
    desfazer: 'Desfazer',
    desfeito: 'Jogo restaurado.',
    exportar: 'Exportar jogos',
    exportado: (quantidade: number, arquivo: string) =>
      `${quantidade} ${quantidade === 1 ? 'jogo exportado' : 'jogos exportados'} no arquivo ${arquivo}.`,
    nadaParaExportar: 'Ainda não há jogos para exportar.',
    exportarFalhou: 'Não foi possível baixar o arquivo.',
    importar: 'Importar jogos',
    importando: 'Importando…',
    resultadoImportacao: (entraram: number, ignorados: number, invalidos: number) => {
      const partes = [`${entraram} ${entraram === 1 ? 'jogo importado' : 'jogos importados'}`];
      partes.push(
        `${ignorados} ${ignorados === 1 ? 'ignorado, pois já estava salvo' : 'ignorados, pois já estavam salvos'}`,
      );
      partes.push(`${invalidos} ${invalidos === 1 ? 'inválido' : 'inválidos'}`);
      return `${partes.join('; ')}.`;
    },
    recusa: {
      grande: 'Arquivo grande demais para ser um backup de jogos. Nada foi importado.',
      malformado: 'O arquivo não é um JSON válido. Nada foi importado.',
      formato: 'O arquivo não é um backup de jogos deste app. Nada foi importado.',
      versao:
        'O arquivo é de uma versão mais nova do app e não pode ser lido aqui. Nada foi importado.',
      leitura: 'Não foi possível ler o arquivo. Nada foi importado.',
    },
  },
  // Conferência automática dos jogos salvos (6.4): Início e Meus jogos.
  novidade: {
    // Início.
    titulo: (loteria: string, concurso: number) =>
      `Concurso ${concurso} saiu: seus jogos salvos da ${loteria}`,
    resumo: (jogos: number, melhor: number) =>
      `${jogos} ${jogos === 1 ? 'jogo conferido' : 'jogos conferidos'}; melhor: ${melhor} ${melhor === 1 ? 'acerto' : 'acertos'}`,
    concursosNovos: (quantidade: number) =>
      `Conferidos contra ${quantidade} concursos novos, desde a última conferência.`,
    faixa: (jogos: number, minimo: number) =>
      `${jogos} ${jogos === 1 ? 'jogo' : 'jogos'} na faixa com prêmio (${minimo} ou mais acertos)`,
    pendente: (jogos: number) =>
      `${jogos} ${jogos === 1 ? 'jogo aguarda' : 'jogos aguardam'} o histórico de concursos para ser conferido. Ele é baixado quando houver rede, ou ao abrir Meus jogos.`,
    verJogos: 'Ver Meus jogos',
    premioNota:
      'O app só conta acertos: não calcula valor de prêmio. Confira o resultado oficial da Caixa.',
    // Meus jogos.
    selo: (concurso: number) => `Novo: concurso ${concurso}`,
    resultado: (concurso: number, acertos: number) =>
      `Concurso ${concurso}: ${acertos} ${acertos === 1 ? 'acerto' : 'acertos'}`,
    resultadosMais: (quantidade: number) =>
      `e mais ${quantidade} ${quantidade === 1 ? 'concurso anterior' : 'concursos anteriores'}`,
    melhorNovos: (acertos: number, concurso: number) =>
      `Melhor entre os novos: ${acertos} ${acertos === 1 ? 'acerto' : 'acertos'} (concurso ${concurso})`,
    naFaixa: (minimo: number) => `Faixa com prêmio: ${minimo} ou mais acertos nesta loteria`,
    jogoPendente: (concurso: number) =>
      `O concurso ${concurso} saiu, mas o histórico ainda não está neste aparelho para conferir. Ele é baixado quando houver rede.`,
    marcarVisto: 'Marcar como visto',
    marcado: 'Jogo marcado como visto.',
    marcarFalhou: 'Não foi possível marcar o jogo como visto.',
    listaResumo: (jogos: number) =>
      `${jogos} ${jogos === 1 ? 'jogo com concurso novo' : 'jogos com concurso novo'}. O destaque some quando você marca como visto.`,
  },
  entenda: {
    titulo: 'Entenda',
    destaque: {
      titulo: 'Este app não prevê sorteios',
      itens: [
        'Cada sorteio é independente dos anteriores.',
        'Nenhum número fica "devendo": uma dezena que não sai há muito tempo não tem mais chance de sair.',
        'Nenhum método, tabela ou índice aumenta a chance de acertar.',
      ],
      fecho: 'O app só organiza o que já aconteceu, para você consultar com clareza.',
    },
    indice: {
      titulo: 'O que é o índice de força',
      paragrafos: [
        'O índice de força resume, em um único número, como cada dezena se comportou nos concursos passados.',
        'Para cada dezena, o app junta nove medidas do histórico (veja abaixo), coloca todas na mesma escala e combina com pesos definidos num perfil. A dezena de maior índice fica em 1º lugar na tabela de força.',
        'A tabela é só uma forma de ordenar o passado. Ela não diz nada sobre o próximo sorteio.',
        'Nos testes feitos pelo projeto, o índice não superou o acaso no critério principal de avaliação.',
      ],
    },
    componentes: {
      resumo: 'Ver as nove medidas do índice',
      intro:
        'Todas olham para o passado, dentro de uma janela de concursos. Quando uma medida não pode ser calculada na janela, a tela mostra "sem dados na janela", e nunca zero.',
    },
    evidencia: {
      titulo: 'O que é a evidência',
      paragrafos: [
        'A evidência diz o quanto os dados convencem de que uma dezena sai com chance diferente da esperada pelo acaso. Vem de um cálculo estatístico sobre o histórico inteiro da loteria.',
        'O grau, de muito fraca a muito forte, mede o quanto os dados convencem, e não o tamanho do desvio. Com muitos concursos, até um desvio minúsculo pode dar evidência forte.',
      ],
      direcoesTitulo: 'As duas direções',
      direcoes: {
        vies: 'Os dados sugerem que a chance da dezena é um pouco diferente da esperada pelo acaso.',
        acaso: 'Os dados favorecem que a dezena siga o acaso.',
      },
      direcoesNota:
        'Leia grau e direção juntos: "forte, viés" é bem diferente de "forte, acaso". Os dados têm um limite de quanta evidência podem dar a favor do acaso, então graus baixos nessa direção são esperados.',
      fecho:
        'Mesmo quando há evidência de viés, o desvio costuma ser minúsculo. Isso não permite prever o próximo sorteio.',
      grausResumo: 'Ver o significado de cada grau',
      grausIntro: 'Os cinco graus, do mais fraco ao mais forte, em cada direção.',
      direcaoRotulo: (direcao: string) => `Direção ${direcao}`,
    },
    leitura: {
      titulo: 'Como ler a tabela de força',
      itens: {
        posicao: {
          rotulo: 'Posição',
          texto:
            'O 1º lugar é a dezena de maior índice no concurso. A posição só compara as dezenas entre si.',
        },
        variacao: {
          rotulo: 'Variação',
          texto:
            'Quantas posições a dezena subiu ou caiu em relação à tabela do concurso anterior. Sem número: não há concurso anterior para comparar.',
        },
        indice: {
          rotulo: 'Índice',
          texto:
            'A nota que combina as nove medidas. Serve para ordenar as dezenas, não para medir chance.',
        },
        evidencia: {
          rotulo: 'Evidência',
          texto:
            'Grau e direção, como explicado acima. Responde se os dados convencem de um desvio do acaso.',
        },
      },
      exemploRotulo: 'Exemplo ilustrativo, não é dado real',
      exemploLeitura:
        'Neste exemplo, a dezena 7 está em 3º lugar, subiu 2 posições e tem evidência moderada de viés. Isso descreve o passado; não quer dizer que ela vá sair.',
    },
    dados: {
      titulo: 'De onde vêm os dados',
      resultadosRotulo: 'Resultados',
      resultadosValor: 'planilhas oficiais da Caixa',
      atualizadoRotulo: 'Atualizado em',
      semAtualizacao: 'ainda não atualizado',
      formatoRotulo: 'Formato dos dados',
      formatoValor: (versao: number) => `versão ${versao}`,
      appRotulo: 'Versão do app',
      appAtualizacaoRotulo: 'Atualização do app',
      appSemAtualizacao: 'nenhuma esperando',
      perfilIntro:
        'O perfil de pesos é a receita que combina as nove medidas no índice. O código do perfil identifica exatamente quais pesos geraram cada tabela.',
      ultimoConcurso: (concurso: number, data: string | null) =>
        data === null ? `${concurso}` : `${concurso} (${data})`,
      ultimoRotulo: 'Último concurso',
      semResultado: 'ainda sem dados',
      perfilRotulo: 'Perfil de pesos',
      janelaRotulo: 'Janela',
      janelaInteira: 'histórico inteiro',
      janela: (n: number) => `últimos ${n} ${n === 1 ? 'concurso' : 'concursos'}`,
      normalizacaoRotulo: 'Normalização',
      featureRotulo: 'Versão das medidas',
      codigoRotulo: 'Código do perfil',
      codigoCompleto: 'Código completo',
      tabelaAusente: 'tabela de força ainda não baixada',
    },
  },
  tema: {
    // `atual` é o rótulo da preferência em uso.
    alternar: (atual: string) => `Tema: ${atual}. Toque para trocar.`,
    automatico: 'automático (segue o aparelho)',
    claro: 'claro',
    escuro: 'escuro',
  },
  aviso: {
    informativo: 'Aviso',
    erro: 'Erro',
  },
  bola: {
    dezena: (numero: number) => `Dezena ${numero}`,
    estados: {
      neutra: '',
      sorteada: 'sorteada',
      destaque: 'em destaque',
      selecionada: 'selecionada',
    },
  },
  volante: {
    // Rótulo do grupo de dezenas de uma loteria.
    grupo: (loteria: string) => `Volante da ${loteria}`,
    dezenaNaFaixa: (numero: number, posicao: number, faixa: number, faixas: number) =>
      `Dezena ${numero}, posição ${posicao}, faixa ${faixa} de ${faixas}`,
    dezenaSemPosicao: (numero: number) => `Dezena ${numero}, sem posição`,
    emDestaque: 'em destaque',
    legenda: 'Faixas de posição no ranking',
    faixa: (faixa: number, de: number, ate: number) => `Faixa ${faixa}: posições ${de} a ${ate}`,
    faixaCurta: (faixa: number) => `Faixa ${faixa}`,
    // Célula do volante da Força: abre o detalhe da dezena.
    celula: (numero: number, posicao: number) => `Dezena ${numero}, ${posicao}º lugar`,
    posicaoCurta: (posicao: number) => `${posicao}º`,
  },
  ranking: {
    posicao: (posicao: number) => `${posicao}º lugar`,
    indice: 'Índice',
    evidencia: 'Evidência',
    semEvidencia: 'sem evidência calculada',
    subiu: (n: number) => `subiu ${n} ${n === 1 ? 'posição' : 'posições'}`,
    caiu: (n: number) => `caiu ${n} ${n === 1 ? 'posição' : 'posições'}`,
    igual: 'manteve a posição',
    semVariacao: 'sem concurso anterior para comparar',
  },
} as const;

// Rótulos dos identificadores enumerados do contrato v1 ("Valores enumerados").
// O tipo Record exige uma entrada por valor conhecido: valor novo no contrato
// quebra o `npm run check` e o teste até receber um rótulo.
export const rotulosEvidenciaGrau: Readonly<Record<GrauEvidencia, string>> = {
  muito_fraca: 'Muito fraca',
  fraca: 'Fraca',
  moderada: 'Moderada',
  forte: 'Forte',
  muito_forte: 'Muito forte',
};

export const rotulosEvidenciaDirecao: Readonly<Record<DirecaoEvidencia, string>> = {
  vies: 'viés',
  acaso: 'acaso',
};

export const rotulosNormalizacao: Readonly<Record<Normalizacao, string>> = {
  zscore: 'Padronização (z-score)',
  minmax: 'Escala de 0 a 1 (mínimo e máximo)',
};

// Frase de cada grau x direção (5 x 2). "Viés" e "acaso" sozinhos não bastam para
// quem lê: a frase diz o que a evidência significa, sem sugerir previsão. Um
// teste exige as 10 frases, sem termos de previsão.
export const frasesEvidencia: Readonly<
  Record<GrauEvidencia, Readonly<Record<DirecaoEvidencia, string>>>
> = {
  muito_fraca: {
    vies: 'Evidência muito fraca de que esta dezena foge do acaso. Os dados quase não permitem concluir nada.',
    acaso:
      'Nada indica que esta dezena fuja do acaso. Isso não prova que ela siga o acaso: só quer dizer que os dados não mostram desvio.',
  },
  fraca: {
    vies: 'Evidência fraca de que esta dezena foge do acaso. É pouco para concluir; o desvio, se existir, é pequeno.',
    acaso:
      'Os dados favorecem, de leve, que esta dezena siga o acaso. Nada indica que ela fuja dele.',
  },
  moderada: {
    vies: 'Evidência moderada de que a chance desta dezena seja um pouco diferente da esperada pelo acaso. O desvio pode ser minúsculo.',
    acaso: 'Os dados favorecem que esta dezena siga o acaso. Nada indica que ela fuja dele.',
  },
  forte: {
    vies: 'Evidência forte de que a chance desta dezena seja um pouco diferente da esperada pelo acaso. Isso não quer dizer que ela vá sair: o desvio pode ser minúsculo.',
    acaso: 'Os dados favorecem fortemente que esta dezena siga o acaso.',
  },
  muito_forte: {
    vies: 'Evidência muito forte de que a chance desta dezena seja um pouco diferente da esperada pelo acaso. Mesmo assim o desvio pode ser minúsculo, e isso não é previsão.',
    acaso: 'Os dados favorecem muito fortemente que esta dezena siga o acaso.',
  },
};

export const fraseEvidencia = (grau: GrauEvidencia, direcao: DirecaoEvidencia): string =>
  frasesEvidencia[grau][direcao];

export interface TextoComponente {
  readonly rotulo: string;
  readonly explicacao: string;
  /** Por que o valor pode faltar (só nos componentes que aceitam `null`). */
  readonly motivoSemDados?: string;
  /** Valor com unidade ("1 vez", "11 concursos"). */
  readonly formatar: (valor: number) => string;
}

const numero = (n: number): string => new Intl.NumberFormat('pt-BR').format(n);
const vezes = (n: number): string => `${numero(n)} ${n === 1 ? 'vez' : 'vezes'}`;
const concursos = (n: number): string => `${numero(n)} ${n === 1 ? 'concurso' : 'concursos'}`;

// Um texto por componente do contrato; o tipo Record exige os 9.
export const textosComponentes: Readonly<Record<ChaveComponente, TextoComponente>> = {
  frequencia: {
    rotulo: 'Frequência na janela',
    explicacao:
      'Quantas vezes a dezena saiu nos concursos da janela. Zero é um valor real: ela não saiu.',
    formatar: vezes,
  },
  maiorEspaco: {
    rotulo: 'Maior espaço sem sair',
    explicacao:
      'O maior número de concursos que ficaram sem a dezena entre duas aparições seguidas, dentro da janela.',
    motivoSemDados: 'Precisa de pelo menos 2 aparições na janela.',
    formatar: concursos,
  },
  maiorEspacoOcorrencias: {
    rotulo: 'Vezes do maior espaço',
    explicacao: 'Quantas vezes esse maior espaço aconteceu na janela.',
    motivoSemDados: 'Depende do maior espaço, que não existe na janela.',
    formatar: vezes,
  },
  menorEspaco: {
    rotulo: 'Menor espaço sem sair',
    explicacao:
      'O menor número de concursos que ficaram sem a dezena entre duas aparições seguidas. Zero quer dizer que ela saiu em concursos seguidos.',
    motivoSemDados: 'Precisa de pelo menos 2 aparições na janela.',
    formatar: concursos,
  },
  menorEspacoOcorrencias: {
    rotulo: 'Vezes do menor espaço',
    explicacao: 'Quantas vezes esse menor espaço aconteceu na janela.',
    motivoSemDados: 'Depende do menor espaço, que não existe na janela.',
    formatar: vezes,
  },
  somaVariacoes: {
    rotulo: 'Oscilação dos espaços',
    explicacao:
      'Soma das diferenças entre espaços seguidos. Quanto maior, mais irregular é o ritmo em que a dezena aparece.',
    motivoSemDados: 'Precisa de pelo menos 3 aparições na janela.',
    formatar: numero,
  },
  espacoFinal: {
    rotulo: 'Concursos desde a última vez',
    explicacao:
      'Quantos concursos se passaram desde a última vez que a dezena saiu. Este número não se limita à janela e pode ser maior que ela.',
    motivoSemDados: 'A dezena nunca saiu.',
    formatar: concursos,
  },
  repeticoesTotal: {
    rotulo: 'Repetições em concursos seguidos',
    explicacao: 'Quantas vezes a dezena saiu em dois concursos seguidos, dentro da janela.',
    formatar: vezes,
  },
  repeticaoAtual: {
    rotulo: 'Sequência atual',
    explicacao:
      'Quantos concursos seguidos a dezena vem saindo, contando para trás a partir do último sorteado. Zero quer dizer que ela não saiu no último.',
    formatar: concursos,
  },
};

export const rotuloEvidenciaGrau = (grau: GrauEvidencia): string => rotulosEvidenciaGrau[grau];
export const rotuloEvidenciaDirecao = (direcao: DirecaoEvidencia): string =>
  rotulosEvidenciaDirecao[direcao];
export const rotuloNormalizacao = (normalizacao: Normalizacao): string =>
  rotulosNormalizacao[normalizacao];

export type IdAba = keyof typeof textos.abas;

export const idsAbas = Object.keys(textos.abas) as IdAba[];
