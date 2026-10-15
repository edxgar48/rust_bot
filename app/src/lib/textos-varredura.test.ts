// Varredura de linguagem (5.4): nenhum texto do app pode sugerir que o índice
// ou o histórico antecipam o sorteio. Todo texto de interface mora em textos.ts,
// então a varredura percorre o módulo inteiro (strings, funções chamadas com
// valores de exemplo, frases de evidência e textos dos componentes).
//
// Lista proibida (comparada sem acento e sem diferenciar maiúsculas):
//  - "palpite", "dica", "sorte", "azar-sorte" e afins: linguagem de aposta;
//  - "vai sair", "vão sair", "vai cair", "deve sair", "tende a sair": afirmam o
//    resultado (a forma negada "não quer dizer que ela vá sair" é permitida: "vá"
//    é subjuntivo e não está na lista);
//  - "quente(s)" e "fria(s)/frio(s)": jargão de "dezenas quentes/frias" que
//    insinua tendência. Estatística de ausência pode usar "atraso/atrasada";
//  - "certeza", "garantia/garantir/garantido", "infalível", "certeira":
//    prometem acerto;
//  - "melhores números/dezenas", "números da sorte", "aposte", "ganhar", "ganhe":
//    incentivam apostar com base no índice.
// Exceção (por frase, não por palavra): "prever/previsão/prevê" só pode aparecer
// numa frase com negação ("não prevê", "nunca prevê", "isso não é previsão").
// "atrasada/atraso" é permitido e não consta da lista.
import { describe, expect, it } from 'vitest';
import {
  fraseEvidencia,
  frasesEvidencia,
  rotulosEvidenciaDirecao,
  rotulosEvidenciaGrau,
  rotulosNormalizacao,
  textos,
  textosComponentes,
} from './textos';

const PROIBIDOS: readonly RegExp[] = [
  /\bpalpites?\b/,
  /\bdicas?\b/,
  /\bsortudo|\bsorte\b|\bsortes\b/,
  /\bvai(?:\s+(?:com\s+certeza|certamente))?\s+(?:sair|cair|ser\s+sorteada)/,
  /\bvao\s+(?:sair|cair)/,
  /\b(?:deve|devem|tende|tendem)\s+(?:a\s+)?(?:sair|cair)/,
  /\bquentes?\b/,
  /\bfri[oa]s?\b/,
  /\bcertezas?\b/,
  /\bgarant/,
  /\binfalive/,
  /\bcerteir[ao]s?\b/,
  /\bmelhores\s+(?:numeros|dezenas)\b/,
  /\bnumeros\s+da\s+sorte\b/,
  /\baposte\b/,
  /\bgan(?:har|he|hador)\b/,
];

const PREVISAO = /\bprev(?:e|er|eem|isao|isoes|isto|isivel|isiveis)\b/;
const NEGACAO = /\b(?:nao|nunca|nem|sem)\b/;

const semAcento = (t: string): string => t.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();

/** Todas as strings do módulo de textos; funções são chamadas com argumentos de exemplo. */
function coletar(valor: unknown, caminho: string, saida: Map<string, string>): void {
  if (typeof valor === 'string') {
    saida.set(caminho, valor);
  } else if (typeof valor === 'function') {
    const f = valor as (...a: unknown[]) => unknown;
    for (const args of [[1, 2, 3, 4], ['x', 2, 3, 4], [null], [1, 'x']]) {
      try {
        const r = f(...args);
        if (typeof r === 'string') saida.set(`${caminho}()`, r);
      } catch {
        /* argumentos de exemplo não servem para esta função; tenta o próximo */
      }
    }
  } else if (valor !== null && typeof valor === 'object') {
    for (const [k, v] of Object.entries(valor)) coletar(v, `${caminho}.${k}`, saida);
  }
}

const todos = new Map<string, string>();
coletar(textos, 'textos', todos);
coletar(frasesEvidencia, 'frasesEvidencia', todos);
coletar(textosComponentes, 'textosComponentes', todos);
coletar(rotulosEvidenciaGrau, 'rotulosEvidenciaGrau', todos);
coletar(rotulosEvidenciaDirecao, 'rotulosEvidenciaDirecao', todos);
coletar(rotulosNormalizacao, 'rotulosNormalizacao', todos);

const frases = (t: string): string[] => t.split(/(?<=[.!?:;])\s+/);

describe('varredura de linguagem: nada sugere previsão', () => {
  it('a varredura de fato percorre muitos textos (guarda contra varredura vazia)', () => {
    expect(todos.size).toBeGreaterThan(150);
    expect(todos.get('textos.entenda.destaque.titulo')).toBe('Este app não prevê sorteios');
    expect(todos.get('textos.avisos.jogoResponsavel')).toContain('responsabilidade');
    expect(fraseEvidencia('forte', 'vies')).toBe(todos.get('frasesEvidencia.forte.vies'));
  });

  it.each(PROIBIDOS.map((r) => [String(r), r] as const))(
    'nenhum texto casa com %s',
    (_nome, regex) => {
      const achados = [...todos]
        .filter(([, t]) => regex.test(semAcento(t)))
        .map(([caminho, t]) => `${caminho}: ${t}`);
      expect(achados).toEqual([]);
    },
  );

  it('"prever/previsão" só aparece em frase com negação', () => {
    const achados: string[] = [];
    for (const [caminho, t] of todos) {
      for (const frase of frases(semAcento(t))) {
        if (PREVISAO.test(frase) && !NEGACAO.test(frase)) achados.push(`${caminho}: ${frase}`);
      }
    }
    expect(achados).toEqual([]);
  });

  it('o próprio detector funciona: pega frases proibidas conhecidas', () => {
    const ruins = [
      'Dezenas quentes que vão sair',
      'Seu palpite da semana',
      'Os melhores números para jogar',
      'Esta dezena vai sair no próximo concurso',
      'O índice garante acerto',
      'Tenha certeza do resultado',
      'Dica: aposte nas dezenas frias',
    ];
    for (const ruim of ruins) {
      const t = semAcento(ruim);
      expect(PROIBIDOS.some((r) => r.test(t))).toBe(true);
    }
    expect(PREVISAO.test(semAcento('O índice prevê o sorteio.'))).toBe(true);
    expect(NEGACAO.test(semAcento('O índice prevê o sorteio.'))).toBe(false);
  });

  it('o detector deixa passar o vocabulário permitido', () => {
    const bons = [
      'Concursos desde a última vez',
      'Esta dezena está atrasada em relação à média do histórico.',
      'Este app não prevê sorteios',
      'Isso não quer dizer que ela vá sair',
      'Cada sorteio é independente',
    ];
    for (const bom of bons) {
      const t = semAcento(bom);
      expect(PROIBIDOS.some((r) => r.test(t))).toBe(false);
    }
  });

  it('as telas com aviso trazem "não prevê" ou "não prevê/independente" e a Entenda traz o de jogo responsável', () => {
    expect(textos.avisos.naoPreveInicio).toContain('não prevê');
    expect(textos.avisos.naoPreveForca).toContain('não prevê');
    expect(textos.avisos.naoPreveDetalhe).toContain('não prevê');
    expect(textos.avisos.naoPreveHistorico).toContain('não prevê');
    expect(textos.avisos.jogoResponsavel).toBe(
      'Jogue com responsabilidade. Apostas são para maiores de 18 anos.',
    );
  });
});
