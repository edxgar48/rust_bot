"""Verificação do PWA no Chrome real (8.1): instalabilidade, service worker e abertura offline.

Pré-requisito: ../publico/v1/index.json (dados reais do contrato). Se não existir, gere com
`cargo run --bin sync -- exportar` na raiz do repositório; o script se recusa a rodar sem ele.

Uso (a partir de app/, depois de `npm run build`; Python 3.12 com `playwright`; Google Chrome instalado):

    py -3.12 scripts/verificar-pwa.py [pasta_de_capturas] [--prefixo /rust_bot/]

Com --prefixo, o build é servido num subcaminho (como no GitHub Pages: a raiz do servidor devolve 404 e
só o prefixo serve o app) e o script confere também o escopo do service worker, o manifesto, os ícones e
o recarregamento em rotas de hash dentro do subcaminho.

O que faz, sem rede externa:
  1. serve dist/ em http://localhost (contexto seguro);
  2. responde a URL do contrato (URL_BASE_PADRAO) com os arquivos reais de ../publico/v1/;
  3. abre o app, espera o service worker controlar a página e os dados chegarem;
  4. pede ao Chrome (CDP) o manifesto lido e os erros de instalabilidade;
  5. confere que o precache do SW tem manifesto e ícones;
  6. coloca o navegador offline, recarrega e confere que a Início abre com os dados do cache;
  7. com --capturas, grava imagens (360 px claro e escuro; Entenda; 1280 px).

Sai com código 1 se qualquer conferência falhar. Não instala o app de verdade nem testa o iOS.
"""

from __future__ import annotations

import functools
import http.server
import json
import mimetypes
import re
import sys
import threading
from pathlib import Path

from playwright.sync_api import sync_playwright

APP = Path(__file__).resolve().parent.parent
DIST = APP / "dist"
PUBLICO_V1 = APP.parent / "publico" / "v1"


def url_base_do_app() -> str:
    """Lê URL_BASE_PADRAO de src/dados/config.ts (fonte única), para não duplicar a URL aqui."""
    texto = (APP / "src" / "dados" / "config.ts").read_text(encoding="utf-8")
    m = re.search(r"URL_BASE_PADRAO\s*=\s*['\"]([^'\"]+)['\"]", texto)
    if not m:
        sys.exit("não achei URL_BASE_PADRAO em src/dados/config.ts")
    return m.group(1)


BASE_CONTRATO = re.compile("^" + re.escape(url_base_do_app()) + "(.*)$")

mimetypes.add_type("application/manifest+json", ".webmanifest")
falhas: list[str] = []
# Com `rede[0]` falso, a URL do contrato falha como numa conexão caída (rota.fulfill ignora set_offline).
rede = [True]


def conferir(condicao: bool, descricao: str) -> None:
    print(("ok    " if condicao else "FALHA ") + descricao)
    if not condicao:
        falhas.append(descricao)


class Silencioso(http.server.SimpleHTTPRequestHandler):
    prefixo = "/"

    def log_message(self, *args) -> None:  # noqa: D401
        pass

    def translate_path(self, path: str) -> str:
        # Fora do prefixo não há nada (404), como num site de projeto do GitHub Pages.
        if not path.startswith(self.prefixo):
            return str(DIST / "__fora_do_prefixo__")
        return super().translate_path("/" + path[len(self.prefixo) :])


def servir(prefixo: str) -> tuple[http.server.ThreadingHTTPServer, str]:
    classe = type("Prefixado", (Silencioso,), {"prefixo": prefixo})
    gerente = functools.partial(classe, directory=str(DIST))
    servidor = http.server.ThreadingHTTPServer(("127.0.0.1", 0), gerente)
    threading.Thread(target=servidor.serve_forever, daemon=True).start()
    return servidor, f"http://localhost:{servidor.server_address[1]}{prefixo}"


def responder_contrato(rota) -> None:
    if not rede[0]:
        rota.abort("internetdisconnected")
        return
    m = BASE_CONTRATO.match(rota.request.url)
    caminho = PUBLICO_V1 / (m.group(1) if m else "")
    if m and caminho.is_file():
        rota.fulfill(
            status=200,
            body=caminho.read_bytes(),
            headers={"content-type": "application/json", "access-control-allow-origin": "*"},
        )
    else:
        rota.fulfill(status=404, body="", headers={"access-control-allow-origin": "*"})


def verificar_subcaminho(pagina, contexto, url: str, prefixo: str) -> None:
    """Conferências extras quando o app é servido fora da raiz (GitHub Pages)."""
    origem = url[: -len(prefixo)]
    escopo = pagina.evaluate("navigator.serviceWorker.getRegistration().then(r => r && r.scope)")
    conferir(escopo == url, f"service worker com escopo {escopo} (esperado {url})")
    manifesto = pagina.evaluate(
        "fetch(document.querySelector('link[rel=manifest]').href).then(r => r.json().then(j => [r.url, j]))"
    )
    conferir(manifesto[0] == url + "manifest.webmanifest", f"manifesto resolve em {manifesto[0]}")
    resposta = contexto.request.get(origem + "/")
    conferir(resposta.status == 404, f"a raiz do servidor fora do prefixo devolve 404 ({resposta.status})")
    for icone in manifesto[1]["icons"]:
        r = contexto.request.get(url + icone["src"])
        conferir(r.status == 200, f"ícone {icone['src']} resolve dentro do prefixo ({r.status})")
    for rota in ("#/forca?loteria=quina", "#/estatisticas", "#/entenda"):
        pagina.goto(url + rota)
        pagina.reload()
        pagina.wait_for_selector("nav", timeout=20000)
        conferir(pagina.evaluate("location.hash") == rota[1:] or pagina.evaluate("location.hash") == rota,
                 f"recarregar em {rota} mantém a rota")
        conferir(pagina.locator("main, #app").first.inner_text().strip() != "", f"{rota}: tela com conteúdo")
    pagina.goto(url)
    pagina.wait_for_selector("text=Concurso", timeout=20000)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    argumentos = sys.argv[1:]
    prefixo = "/"
    if "--prefixo" in argumentos:
        i = argumentos.index("--prefixo")
        prefixo = argumentos[i + 1] if i + 1 < len(argumentos) else ""
        del argumentos[i : i + 2]
        if not (prefixo.startswith("/") and prefixo.endswith("/")):
            print("--prefixo deve começar e terminar com / (ex.: /rust_bot/)")
            return 1
    capturas = Path(argumentos[0]) if argumentos else None
    if capturas:
        capturas.mkdir(parents=True, exist_ok=True)
    if not (PUBLICO_V1 / "index.json").is_file():
        print(
            f"{PUBLICO_V1 / 'index.json'} não existe: rode `cargo run --bin sync -- exportar` "
            "na raiz do repositório antes"
        )
        return 1
    if not (DIST / "index.html").is_file():
        print("dist/ não existe: rode `npm run build` antes")
        return 1
    servidor, url = servir(prefixo)
    with sync_playwright() as p:
        navegador = p.chromium.launch(channel="chrome")
        contexto = navegador.new_context(viewport={"width": 360, "height": 740}, locale="pt-BR")
        contexto.route(BASE_CONTRATO, responder_contrato)
        pagina = contexto.new_page()
        erros: list[str] = []
        pagina.on("pageerror", lambda e: erros.append(str(e)))

        pagina.goto(url)
        pagina.wait_for_selector("text=Concurso", timeout=20000)
        pagina.evaluate("navigator.serviceWorker.ready.then(() => true)")
        pagina.reload()  # a segunda carga já é controlada pelo SW
        pagina.wait_for_selector("text=Concurso", timeout=20000)
        conferir(pagina.evaluate("!!navigator.serviceWorker.controller"), "service worker controla a página")

        # Instalabilidade e manifesto, como o Chrome os enxerga.
        sessao = contexto.new_cdp_session(pagina)
        erros_instalacao = sessao.send("Page.getInstallabilityErrors")["installabilityErrors"]
        # "in-incognito" é do contexto temporário do Playwright, não do app.
        erros_instalacao = [e for e in erros_instalacao if e["errorId"] != "in-incognito"]
        conferir(erros_instalacao == [], f"Chrome não aponta erro de instalabilidade {erros_instalacao}")
        manifesto = sessao.send("Page.getAppManifest")
        conferir(manifesto.get("errors", []) == [], f"manifesto sem erros de leitura {manifesto.get('errors')}")
        dados = json.loads(manifesto["data"])
        conferir(dados["start_url"] == url or dados["start_url"] == "./", f"start_url {dados['start_url']}")
        conferir(dados["display"] == "standalone", "display standalone")

        if prefixo != "/":
            verificar_subcaminho(pagina, contexto, url, prefixo)

        # Precache do SW: manifesto e ícones.
        precache = pagina.evaluate(
            """async () => {
              const nomes = await caches.keys();
              const urls = [];
              for (const n of nomes) for (const r of await (await caches.open(n)).keys()) urls.push(r.url);
              return urls;
            }"""
        )
        for nome in (
            "manifest.webmanifest",
            "icone-192.png",
            "icone-512.png",
            "icone-maskable-192.png",
            "icone-maskable-512.png",
            "apple-touch-icon.png",
            "favicon.ico",
            "icone.svg",
            "index.html",
        ):
            conferir(any(u.split("?")[0].endswith("/" + nome) for u in precache), f"precache contém {nome}")

        # Offline: recarrega sem rede; os dados vêm do IndexedDB.
        contexto.set_offline(True)
        rede[0] = False
        pagina.reload()
        pagina.wait_for_selector("text=Concurso", timeout=20000)
        texto = pagina.inner_text("body")
        conferir("Concurso" in texto and "Lotofácil" in texto, "offline: Início abre com resultados do cache")
        conferir("Sem rede" in texto or "sem rede" in texto, "offline: a tela diz que está sem rede")
        conferir(not erros, f"sem erros de página {erros}")

        if capturas:
            pagina.screenshot(path=str(capturas / "inicio-offline-360.png"))
            pagina.goto(url + "#/entenda")
            pagina.get_by_text("Instalar o app").last.scroll_into_view_if_needed()
            pagina.screenshot(path=str(capturas / "entenda-360.png"), full_page=True)
        contexto.set_offline(False)
        rede[0] = True

        # Aviso de instalação no iOS Safari (UA simulado) e no modo claro/escuro.
        ios = navegador.new_context(
            viewport={"width": 360, "height": 740},
            locale="pt-BR",
            user_agent=(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 "
                "(KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1"
            ),
            color_scheme="dark",
        )
        ios.route(BASE_CONTRATO, responder_contrato)
        pi = ios.new_page()
        pi.goto(url)
        pi.wait_for_selector("text=Concurso", timeout=20000)
        conferir(pi.get_by_text("Adicionar à Tela de Início").count() == 1, "iOS Safari simulado: instrução aparece")
        if capturas:
            pi.screenshot(path=str(capturas / "inicio-ios-escuro-360.png"), full_page=True)
        pi.get_by_role("button", name="Dispensar o aviso de instalação").click()
        conferir(pi.get_by_text("Adicionar à Tela de Início").count() == 0, "dispensar esconde o aviso")
        pi.reload()
        pi.wait_for_selector("text=Concurso", timeout=20000)
        conferir(pi.get_by_text("Adicionar à Tela de Início").count() == 0, "a dispensa persiste após recarregar")

        if capturas:
            largo = navegador.new_context(viewport={"width": 1280, "height": 800}, locale="pt-BR")
            largo.route(BASE_CONTRATO, responder_contrato)
            pl = largo.new_page()
            pl.goto(url + "#/entenda")
            pl.wait_for_selector("text=Instalar o app", timeout=20000)
            pl.screenshot(path=str(capturas / "entenda-1280.png"), full_page=True)
        navegador.close()
    servidor.shutdown()
    print(f"\n{len(falhas)} falha(s)")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
