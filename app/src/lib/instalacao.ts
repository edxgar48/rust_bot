// Instalação do app (8.1): lógica pura e tolerante a falha. A tela só decide
// com o que sai daqui. Nada sai do aparelho.
import { armazenamentoLocal, type EscritorArmazenamento, type LeitorArmazenamento } from './tema';

/** Chave do localStorage com a dispensa do aviso de instalação. */
export const CHAVE_INSTALACAO_DISPENSADA = 'instalar.dispensado';

/** Evento não padronizado do Chrome/Edge/Android: o navegador oferece a instalação. */
export interface EventoInstalacao extends Event {
  prompt: () => Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

export interface AmbienteNavegador {
  userAgent: string;
  platform: string;
  maxTouchPoints: number;
}

/**
 * iOS (iPhone, iPad, iPod; iPadOS 13+ se apresenta como Mac, mas tem toque) com o
 * Safari: no iOS, navegadores de terceiros (Chrome, Firefox, Edge, Opera, Google app)
 * têm outro menu e ficam de fora da instrução.
 */
export function ehIosSafari(ambiente: AmbienteNavegador): boolean {
  const ua = ambiente.userAgent;
  const ios =
    /iPhone|iPad|iPod/.test(ua) ||
    (ambiente.platform === 'MacIntel' && ambiente.maxTouchPoints > 1);
  if (!ios) return false;
  if (/CriOS|FxiOS|EdgiOS|OPiOS|OPT\/|GSA\/|DuckDuckGo|YaBrowser/.test(ua)) return false;
  return /Safari\//.test(ua);
}

export interface AmbienteInstalado {
  matchMedia?: (consulta: string) => { matches: boolean };
  /** O `navigator` real (no Safari do iOS ele tem `standalone`, fora dos tipos do DOM). */
  navigator?: object;
}

/** Já aberto como app instalado (Android/desktop: display-mode; iOS: navigator.standalone). */
export function estaInstalado(ambiente: AmbienteInstalado): boolean {
  try {
    if ((ambiente.navigator as { standalone?: unknown } | undefined)?.standalone === true)
      return true;
    return ambiente.matchMedia?.('(display-mode: standalone)').matches === true;
  } catch {
    return false;
  }
}

/** `true` só se a dispensa foi gravada; falha de leitura = não dispensado. */
export function lerDispensa(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): boolean {
  try {
    return armazenamento?.getItem(CHAVE_INSTALACAO_DISPENSADA) === '1';
  } catch {
    return false;
  }
}

/** Devolve `false` se não foi possível lembrar (o aviso some só nesta sessão). */
export function gravarDispensa(
  armazenamento: Pick<EscritorArmazenamento, 'setItem'> | null = armazenamentoLocal(),
): boolean {
  try {
    if (!armazenamento) return false;
    armazenamento.setItem(CHAVE_INSTALACAO_DISPENSADA, '1');
    return true;
  } catch {
    return false;
  }
}

export type OfertaInstalacao = 'nenhuma' | 'botao' | 'ios';

export interface EntradaOferta {
  instalado: boolean;
  dispensado: boolean;
  /** O navegador entregou o evento `beforeinstallprompt` e ele ainda vale. */
  eventoDisponivel: boolean;
  ios: boolean;
}

/** O que mostrar: já instalado ou dispensado = nada; evento do navegador = botão; iOS Safari = instrução. */
export function decidirOferta(e: EntradaOferta): OfertaInstalacao {
  if (e.instalado || e.dispensado) return 'nenhuma';
  if (e.eventoDisponivel) return 'botao';
  if (e.ios) return 'ios';
  return 'nenhuma';
}

export type ResultadoInstalacao = 'aceito' | 'recusado' | 'indisponivel';

type AlvoEventos = Pick<Window, 'addEventListener' | 'removeEventListener'>;

/**
 * Guarda o `beforeinstallprompt` (que chega cedo, antes de qualquer tela) e avisa
 * quem assinou. O evento só pode ser usado uma vez.
 */
export class ControleInstalacao {
  #evento: EventoInstalacao | null = null;
  #instaladoAgora = false;
  readonly #ouvintes = new Set<() => void>();
  readonly #aoPerguntar = (e: Event): void => {
    e.preventDefault(); // sem o mini-aviso do navegador: a oferta é da tela
    this.#evento = e as EventoInstalacao;
    this.#avisar();
  };
  readonly #aoInstalar = (): void => {
    this.#evento = null;
    this.#instaladoAgora = true;
    this.#avisar();
  };

  constructor(private readonly alvo: AlvoEventos) {
    alvo.addEventListener('beforeinstallprompt', this.#aoPerguntar);
    alvo.addEventListener('appinstalled', this.#aoInstalar);
  }

  get eventoDisponivel(): boolean {
    return this.#evento !== null;
  }

  /** O app foi instalado durante esta sessão (evento `appinstalled`). */
  get instaladoAgora(): boolean {
    return this.#instaladoAgora;
  }

  /** Chama `ouvinte` a cada mudança; devolve a função que cancela. */
  assinar(ouvinte: () => void): () => void {
    this.#ouvintes.add(ouvinte);
    return () => this.#ouvintes.delete(ouvinte);
  }

  async instalar(): Promise<ResultadoInstalacao> {
    const evento = this.#evento;
    if (!evento) return 'indisponivel';
    this.#evento = null; // uso único, aceito ou não
    this.#avisar();
    try {
      await evento.prompt();
      const { outcome } = await evento.userChoice;
      return outcome === 'accepted' ? 'aceito' : 'recusado';
    } catch {
      return 'indisponivel';
    }
  }

  encerrar(): void {
    this.alvo.removeEventListener('beforeinstallprompt', this.#aoPerguntar);
    this.alvo.removeEventListener('appinstalled', this.#aoInstalar);
    this.#ouvintes.clear();
  }

  #avisar(): void {
    for (const ouvinte of [...this.#ouvintes]) ouvinte();
  }
}

let global: ControleInstalacao | null = null;

/** Controle único da página; criar cedo (main.ts) para não perder o evento. */
export function controleGlobal(): ControleInstalacao | null {
  if (typeof window === 'undefined') return null;
  global ??= new ControleInstalacao(window);
  return global;
}
