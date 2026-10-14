// SHA-256 pelos bytes exatos recebidos (Web Crypto), em hexadecimal minúsculo.
export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const resumo = await globalThis.crypto.subtle.digest('SHA-256', bytes as BufferSource);
  return Array.from(new Uint8Array(resumo), (b) => b.toString(16).padStart(2, '0')).join('');
}
