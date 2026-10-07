"""Experimentos de ML do rust_spider (Fase 1.5).

Python só experimenta e propõe perfis de pesos; a produção é Rust. Este
pacote lê os datasets gerados por ``sync dataset`` (sempre pelo manifesto),
refaz o índice de um perfil ``.toml`` e o avalia contra o acaso com as mesmas
regras do ``sync avaliar`` (``docs/PLANEJAMENTO.md``, 1b, 1c e 1.5).
"""

from .avaliacao import (
    AvaliacaoFaixa,
    AvaliacaoPerfil,
    Comparacao,
    ErroAvaliacao,
    avaliar_perfil,
    dividir_em_dobras,
)
from .dataset import COLUNAS, COMPONENTES, Dataset, ErroDataset, ler_dataset
from .indice import ErroIndice, calcular_indice
from .perfil import ErroPerfil, Perfil, carregar_perfil

__all__ = [
    "COLUNAS",
    "COMPONENTES",
    "AvaliacaoFaixa",
    "AvaliacaoPerfil",
    "Comparacao",
    "Dataset",
    "ErroAvaliacao",
    "ErroDataset",
    "ErroIndice",
    "ErroPerfil",
    "Perfil",
    "avaliar_perfil",
    "calcular_indice",
    "carregar_perfil",
    "dividir_em_dobras",
    "ler_dataset",
]
