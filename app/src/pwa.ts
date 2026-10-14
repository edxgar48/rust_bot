// Registro do service worker (só no build; em dev fica desativado). A nova
// versão é instalada em segundo plano e vale na próxima abertura: nunca
// recarrega a página sob o usuário.
export function registrarServiceWorker(aoHaverAtualizacao: () => void): void {
  if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return;
  void import('virtual:pwa-register')
    .then(({ registerSW }) => {
      registerSW({ immediate: true, onNeedRefresh: aoHaverAtualizacao });
    })
    .catch(() => undefined);
}
