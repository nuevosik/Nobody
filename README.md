# Nobody

Daemon de notificações para Wayland, escrito em Rust. Ele assume o nome
`org.freedesktop.Notifications` no session bus e exibe notificações em uma
camada GPUI no canto superior direito da tela.

## Executar

O Nobody precisa de uma sessão Wayland com Layer Shell e de um session bus D-Bus.
Pare qualquer daemon que já possua o nome de notificações (por exemplo, mako) e
execute:

```bash
cargo run --release
```

## Comportamento atual

- Implementa `Notify`, `CloseNotification`, `GetCapabilities` e
  `GetServerInformation` do protocolo Desktop Notifications.
- Mantém até 12 notificações ativas e renderiza as cinco mais recentes.
- Emite `NotificationClosed` para expiração, descarte pelo usuário,
  `CloseNotification` e descarte por capacidade.
- Substitui uma notificação de forma atômica quando recebe `replaces_id`.
- Usa o timeout pedido pelo cliente; `-1` usa o padrão do servidor (5 segundos),
  `0` nunca expira e notificações críticas nunca expiram automaticamente.
- Aceita caminhos/nome de ícone e `desktop-entry`. A busca é limitada a locais
  conhecidos para não varrer o disco em cada notificação.
- Suporta corpo de texto. Markup é removido antes da renderização; ações,
  `image-data` e histórico persistente ainda não são suportados.
- Clique, Enter, Espaço ou Escape dispensam uma notificação. Defina
  `PREFERS_REDUCED_MOTION=1` para desativar animações.

## Arquitetura

```
src/
├── main.rs          inicialização GPUI
├── daemon.rs        interface D-Bus e sinais
├── queue.rs         estado compartilhado, IDs e pedidos de fechamento
├── provider.rs      política de timeout e orquestração da UI
├── icons.rs         resolução limitada e cache de ícones
├── state.rs         modelos de domínio
└── ui/
    ├── stack.rs     Layer Shell, sincronização e interação
    ├── popup.rs     cartão de notificação e acessibilidade
    └── anim.rs      animações
```

## Desenvolvimento

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Próximos passos

- [ ] Ações de notificação e `ActionInvoked`.
- [ ] Suporte a `image-data` e temas de ícones completos.
- [ ] Configuração de aparência e timeout por urgência.
- [ ] Histórico/persistência de notificações.
