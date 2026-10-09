"""Experimentos de ML do rust_spider (Fase 1.5).

Python só experimenta e propõe perfis de pesos; a produção é Rust. Este
pacote lê os datasets gerados por ``sync dataset`` (sempre pelo manifesto),
refaz o índice de um perfil ``.toml``, o avalia contra o acaso com as mesmas
regras do ``sync avaliar`` e busca pesos com walk-forward, propondo perfis
``busca_v<N>.toml`` (``docs/PLANEJAMENTO.md``, 1b, 1c e 1.5).
"""

from .avaliacao import (
    AvaliacaoFaixa,
    AvaliacaoPerfil,
    Comparacao,
    ErroAvaliacao,
    avaliar_perfil,
    dividir_em_dobras,
)
from .busca import Candidato, ErroBusca, ResultadoBusca, buscar, gerar_candidatos, gravar_perfil
from .dataset import COLUNAS, COMPONENTES, CORTES_BUSCA, Dataset, ErroDataset, ler_dataset
from .indice import ErroIndice, calcular_indice
from .perfil import ErroPerfil, Perfil, carregar_perfil

__all__ = [
    "COLUNAS",
    "COMPONENTES",
    "CORTES_BUSCA",
    "AvaliacaoFaixa",
    "Candidato",
    "AvaliacaoPerfil",
    "Comparacao",
    "Dataset",
    "ErroAvaliacao",
    "ErroBusca",
    "ErroDataset",
    "ErroIndice",
    "ErroPerfil",
    "Perfil",
    "ResultadoBusca",
    "avaliar_perfil",
    "buscar",
    "calcular_indice",
    "carregar_perfil",
    "dividir_em_dobras",
    "gerar_candidatos",
    "gravar_perfil",
    "ler_dataset",
]
