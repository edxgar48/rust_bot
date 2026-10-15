"""Gera os ícones PNG do PWA a partir de public/icone.svg (fonte única).

Uso (a partir de app/, Python 3.12 com `playwright` e `Pillow`):

    py -3.12 scripts/gerar-icones.py

Renderiza o SVG no Google Chrome instalado (Playwright, channel="chrome") e grava em public/:

  icone-192.png, icone-512.png            purpose "any" (cantos arredondados, fundo transparente)
  icone-maskable-192.png, -512.png        purpose "maskable" (sangria total, sem cantos; o
                                          desenho fica dentro da zona segura de 80%)
  apple-touch-icon.png (180x180)          iOS não aceita transparência: sangria total
  favicon-32.png, favicon.ico (16/32/48)

Os PNG são versionados; o build NÃO roda este script. Rode-o só se o
icone.svg mudar. A saída depende da fonte sans-serif do sistema (o "25").
"""

from __future__ import annotations

import io
import re
import sys
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

PUBLIC = Path(__file__).resolve().parent.parent / "public"
ORIGEM = PUBLIC / "icone.svg"

# Fração do lado ocupada pelo desenho nos ícones maskable. O círculo do
# icone.svg tem raio 150/512 (diâmetro ~59%), então já cabe com folga na zona
# segura (círculo de 80% do lado); a conferência abaixo garante isso.
ZONA_SEGURA = 0.80


def svg_sem_cantos(svg: str) -> str:
    """Remove o arredondamento do fundo: a plataforma aplica a máscara."""
    novo, n = re.subn(r'(<rect[^>]*?)\s+rx="\d+"', r"\1", svg, count=1)
    if n != 1:
        raise SystemExit("icone.svg: não achei o rx do fundo")
    return novo


def conferir_zona_segura(svg: str) -> None:
    m = re.search(r'<circle[^>]*\sr="(\d+)"', svg)
    v = re.search(r'viewBox="0 0 (\d+) \1"', svg)
    if not m or not v:
        raise SystemExit("icone.svg: círculo ou viewBox não encontrados")
    raio = int(m.group(1)) / int(v.group(1))
    if raio > ZONA_SEGURA / 2:
        raise SystemExit(f"desenho fora da zona segura de {ZONA_SEGURA:.0%}")


def renderizar(pagina, svg: str, lado: int, transparente: bool) -> Image.Image:
    html = (
        "<!doctype html><meta charset=utf-8><style>"
        "html,body{margin:0;background:transparent}"
        f"svg{{display:block;width:{lado}px;height:{lado}px}}</style>{svg}"
    )
    pagina.set_viewport_size({"width": lado, "height": lado})
    pagina.set_content(html)
    png = pagina.screenshot(omit_background=transparente, clip={"x": 0, "y": 0, "width": lado, "height": lado})
    return Image.open(io.BytesIO(png)).convert("RGBA")


def main() -> int:
    svg = ORIGEM.read_text(encoding="utf-8")
    conferir_zona_segura(svg)
    opaco = svg_sem_cantos(svg)
    with sync_playwright() as p:
        navegador = p.chromium.launch(channel="chrome")
        pagina = navegador.new_page(device_scale_factor=1)
        saidas: dict[str, Image.Image] = {}
        for lado in (192, 512):
            saidas[f"icone-{lado}.png"] = renderizar(pagina, svg, lado, True)
            saidas[f"icone-maskable-{lado}.png"] = renderizar(pagina, opaco, lado, False)
        saidas["apple-touch-icon.png"] = renderizar(pagina, opaco, 180, False)
        # O Chrome não cria janelas minúsculas: reduz a partir de 256 px.
        icone_grande = renderizar(pagina, svg, 256, True)
        saidas["favicon-32.png"] = icone_grande.resize((32, 32), Image.LANCZOS)
        navegador.close()
    for nome, imagem in saidas.items():
        imagem.save(PUBLIC / nome, optimize=True)
        print("gravado", nome, imagem.size)
    icone_grande.save(PUBLIC / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)])
    print("gravado favicon.ico")
    return 0


if __name__ == "__main__":
    sys.exit(main())
