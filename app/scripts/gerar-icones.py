"""Gera os ícones PNG do PWA a partir de arte/Logo_app.png (fonte única).

Uso (a partir de app/, Python 3.12 com `Pillow`):

    py -3.12 scripts/gerar-icones.py

A arte original (1408x768) traz o ícone arredondado no centro de um fundo escuro.
O script recorta o quadrado do ícone e grava em public/:

  icone-192.png, icone-512.png            purpose "any" (cantos arredondados, fundo transparente)
  icone-maskable-192.png, -512.png        purpose "maskable" (sangria total, sem cantos; o
                                          desenho fica dentro da zona segura de 80%)
  apple-touch-icon.png (180x180)          iOS não aceita transparência: sangria total
  favicon-32.png, favicon.ico (16/32/48)  só o emblema (o texto não é legível em 32 px)

Os PNG são versionados; o build NÃO roda este script. Rode-o só se a arte mudar.
"""

from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

RAIZ = Path(__file__).resolve().parent.parent
PUBLIC = RAIZ / "public"
ORIGEM = RAIZ / "arte" / "Logo_app.png"

# Quadrado do ícone dentro da arte (esquerda, topo, direita, base), medido na imagem.
ICONE = (379, 63, 1030, 708)
# Fração do raio de canto do ícone original em relação ao lado.
RAIO_CANTO = 0.21
# Emblema (estrela + espiral) usado no favicon.
EMBLEMA = (540, 140, 880, 480)
# Zona segura dos ícones maskable: círculo com 80% do lado.
ZONA_SEGURA = 0.80


def recorte_central(arte: Image.Image, caixa: tuple[int, int, int, int], recuo: int) -> Image.Image:
    """Recorte quadrado e centralizado em `caixa`, recuado `recuo` px de cada lado."""
    e, t, d, b = caixa
    lado = min(d - e, b - t) - 2 * recuo
    cx, cy = (e + d) // 2, (t + b) // 2
    meio = lado // 2
    return arte.crop((cx - meio, cy - meio, cx - meio + lado, cy - meio + lado))


def com_cantos(img: Image.Image) -> Image.Image:
    """Aplica cantos arredondados e transparência fora deles (purpose "any")."""
    ampliacao = 4  # máscara em alta resolução para suavizar a borda
    lado = img.width * ampliacao
    mascara = Image.new("L", (lado, lado), 0)
    ImageDraw.Draw(mascara).rounded_rectangle(
        (0, 0, lado - 1, lado - 1), radius=int(lado * RAIO_CANTO), fill=255
    )
    mascara = mascara.resize(img.size, Image.LANCZOS)
    saida = img.convert("RGBA")
    saida.putalpha(mascara)
    return saida


def maskable(arte: Image.Image) -> Image.Image:
    """Ícone de sangria total com o desenho dentro da zona segura de 80%."""
    miolo = recorte_central(arte, ICONE, recuo=40)
    # Raio do desenho (canto do texto) ≈ 320 px na arte; precisa caber em 40% do lado.
    lado = 800
    # Fundo liso na cor média da borda do recorte; as bordas do recorte são esmaecidas para não
    # deixar emenda (o brilho do desenho não chega até elas).
    borda = [miolo.getpixel((i, j)) for i in range(0, miolo.width, 8) for j in (0, miolo.height - 1)]
    borda += [miolo.getpixel((i, j)) for j in range(0, miolo.height, 8) for i in (0, miolo.width - 1)]
    cor = tuple(sum(c[k] for c in borda) // len(borda) for k in range(3))
    fundo = Image.new("RGB", (lado, lado), cor)
    mascara = Image.new("L", miolo.size, 0)
    ImageDraw.Draw(mascara).rectangle((10, 10, miolo.width - 11, miolo.height - 11), fill=255)
    mascara = mascara.filter(ImageFilter.GaussianBlur(5))
    fundo.paste(miolo, ((lado - miolo.width) // 2, (lado - miolo.height) // 2), mascara)
    raio_max = 320
    if raio_max > ZONA_SEGURA / 2 * lado:
        raise SystemExit(f"desenho fora da zona segura de {ZONA_SEGURA:.0%}")
    return fundo


def main() -> int:
    arte = Image.open(ORIGEM).convert("RGB")
    cheio = recorte_central(arte, ICONE, recuo=3)
    sangria = maskable(arte)
    apple = recorte_central(arte, ICONE, recuo=50)
    emblema = arte.crop(EMBLEMA)

    saidas: dict[str, Image.Image] = {}
    for lado in (192, 512):
        saidas[f"icone-{lado}.png"] = com_cantos(cheio.resize((lado, lado), Image.LANCZOS))
        saidas[f"icone-maskable-{lado}.png"] = sangria.resize((lado, lado), Image.LANCZOS).convert("RGB")
    saidas["apple-touch-icon.png"] = apple.resize((180, 180), Image.LANCZOS)
    grande = emblema.resize((256, 256), Image.LANCZOS)
    saidas["favicon-32.png"] = grande.resize((32, 32), Image.LANCZOS)

    for nome, imagem in saidas.items():
        imagem.save(PUBLIC / nome, optimize=True)
        print("gravado", nome, imagem.size)
    grande.save(PUBLIC / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)])
    print("gravado favicon.ico")
    return 0


if __name__ == "__main__":
    sys.exit(main())
