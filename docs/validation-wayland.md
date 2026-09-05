# Validação no desktop — 2026-09-05

Ambiente: Arch Linux, Hyprland, Wayland (`wayland-1`).

## Registro da verificação inicial

- `~/.local/bin/nobody` e `target/release/nobody` têm o mesmo SHA-256:
  `c7c9996bc477604eb46698cdbfa3427af1274c1ff299910f737dfa84f594fc4d`.
- `nobody center toggle` abre e fecha o painel.
- A consulta `or` deixada pela sessão anterior permanece ao reabrir.
  Isso confirma preservação da consulta, não valida como ela foi digitada.
- `nobody dnd off` atualiza visualmente o botão do painel para `off`.
- Com a central fechada e DND desligado, uma notificação de teste aparece.
- Com DND ligado, uma notificação de teste fica oculta.
- Ao terminar, central fechada, DND manual desligado e layout `us`,
  sem variante. O DND ligado era um efeito acidental documentado na sessão
  anterior. O histórico existente foi preservado.
- `cargo fmt --check`, `cargo check --all-targets`,
  `cargo clippy --all-targets -- -D warnings` e
  `cargo test --all-targets` passaram: 139 testes.

## Atualização após revisão

- Fcitx5 está instalado em `/usr/bin/fcitx5` e em execução. A ausência de
  IME registrada na verificação inicial não é mais um bloqueio.
- Corrigida a conversão UTF-16: posições no meio de um par substituto são
  arredondadas para o início do caractere, sem consumir o restante da busca.
  Teste de regressão cobre limites parciais e substituição completa de emoji.
- `cargo test --all-targets`: 140 testes passaram. `cargo fmt --check` e
  `cargo clippy --all-targets -- -D warnings` também passaram.
- A integração Waybar foi conferida visualmente: ícone de 16 px, tooltip e
  clique para abrir e fechar a central. O painel respeita a área da barra.

## Validação ainda pendente

O provedor de acessibilidade do Orca continua listando apenas Waybar.
`get-app-state --app nobody --restore-window` retorna `app_not_found`, e
`capabilities` informa `windows.focus: false`. Assim, esta nova tentativa
não confirmou um receptor de texto no Nobody e não enviou teclas globais.

Portanto, composição e confirmação via IME real, cancelamento, Backspace
durante composição, Shift, Ctrl+V, emoji e fechar/reabrir durante composição
continuam sem validação manual conclusiva. Os testes da lógica IME não
substituem esses testes. Também falta confirmar foco de teclado ao abrir
a central por atalho no compositor.

Para concluir: com Fcitx5 ativo, focar manualmente a busca,
executar esses casos e verificar texto final, ausência de duplicação ou
preedição residual e restauração do foco. Registrar compositor, IME e método
de entrada utilizados. Se houver falha, corrigir e repetir o caso.
