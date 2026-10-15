// Entrega de um arquivo de texto ao usuário (Blob + a[download]) e leitura de um
// arquivo escolhido. Sem rede; tudo acontece no aparelho.

/** Baixa `conteudo` como arquivo. Devolve `false` se o navegador não conseguiu. */
export function baixarTexto(nome: string, conteudo: string, tipo = 'application/json'): boolean {
  try {
    const url = URL.createObjectURL(new Blob([conteudo], { type: `${tipo};charset=utf-8` }));
    const ancora = document.createElement('a');
    ancora.href = url;
    ancora.download = nome;
    ancora.rel = 'noopener';
    ancora.style.display = 'none';
    document.body.appendChild(ancora);
    ancora.click();
    ancora.remove();
    // O download já começou; soltar o objeto depois evita vazamento sem cancelá-lo.
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
    return true;
  } catch {
    return false;
  }
}

/** Texto de um arquivo escolhido pelo usuário. Rejeita se a leitura falhar. */
export function lerTextoDoArquivo(arquivo: Blob): Promise<string> {
  if (typeof arquivo.text === 'function') return arquivo.text();
  return new Promise((resolver, rejeitar) => {
    const leitor = new FileReader();
    leitor.onload = () => resolver(String(leitor.result ?? ''));
    leitor.onerror = () => rejeitar(leitor.error ?? new Error('Falha ao ler o arquivo.'));
    leitor.readAsText(arquivo);
  });
}
