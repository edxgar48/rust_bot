// Datas de sorteio do contrato (AAAA-MM-DD): são datas de calendário, sem hora
// nem fuso. Por isso a formatação é só troca de texto, nunca passa por `Date`
// (que interpretaria "2026-10-13" como meia-noite UTC e, em fusos a oeste,
// mostraria o dia anterior).

/** "2026-10-13" -> "13/10/2026". Texto fora do formato volta como veio. */
export function formatarDataSorteio(data: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(data);
  return m ? `${m[3]}/${m[2]}/${m[1]}` : data;
}
