# Arquitetura — Nobody

Daemon `org.freedesktop.Notifications` em Rust (GPUI + LayerShell + zbus),
organizado em Clean Architecture: `domain`, `application`, `infrastructure`,
`presentation`. A biblioteca mora em `src/lib.rs` (re-exporta as quatro
camadas); `src/main.rs` é só bootstrap fino que compõe as camadas via a
`Queue` compartilhada.

## Árvore

```
src/
  main.rs
  lib.rs
  domain/
    mod.rs
    notice.rs
    queue.rs
    ids.rs
    close.rs
  application/
    mod.rs
    policy.rs
    commands.rs
    clock.rs
  infrastructure/
    mod.rs
    dbus/
      mod.rs
      daemon.rs
      validation.rs
      markup.rs
      host.rs
    icons/
      mod.rs
      resolver.rs
      lookup.rs
      desktop.rs
      cache.rs
  presentation/
    mod.rs
    theme.rs
    shell/
      mod.rs
      window.rs
      geometry.rs
      feed.rs
      popup.rs
      anim.rs
```

(`lib.rs` não existia na spec original — segura os `pub mod` para os testes
de integração usarem `nobody::...`; `main.rs` só faz bootstrap.)

Testes de integração em `tests/` usam só a API pública:

- `tests/queue_invariants.rs` — fila capa em `KEEP=12`, reporta evictados,
  `replaces_id` fantasma não squatta ID arbitrário.
- `tests/dbus_contract.rs` — `GetCapabilities` anuncia só `body` +
  `icon-static` (nunca `actions` nem `body-markup`).

## Dependency Rule

`domain` não conhece ninguém; `application` só conhece `domain`;
`infrastructure` e `presentation` conhecem para dentro, nunca entre si —
`presentation` nunca importa `infrastructure` e vice-versa.

Guards no CI (devem sair vazios):

```bash
! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation
! rg -n 'presentation::' src/infrastructure
```

## Onde pôr código novo

| Mudança | Lugar |
|---|---|
| Regra nova de timeout (expiração, default, urgência) | `application/policy.rs` |
| Novo formato de ícone (lookup, desktop entry, cache) | `infrastructure/icons/` (`resolver.rs`, `lookup.rs`, `desktop.rs`, `cache.rs`) |
| Novo layout (posição, geometria, região de input) | `presentation/shell/geometry.rs` |
| Novo comando/orquestração sobre a fila | `application/commands.rs` |
| Entidade, ID, motivo de fechamento | `domain/` (`notice.rs`, `ids.rs`, `close.rs`) |
| Validação/truncação de payload D-Bus, markup | `infrastructure/dbus/` (`validation.rs`, `markup.rs`) |
| Widget, cartão, animação, tema | `presentation/shell/` (`popup.rs`, `anim.rs`), `presentation/theme.rs` |

Data flow resumido: `Notify` expira pendentes via `commands::expire` →
valida/trunca → `strip_markup` → resolve ícone em thread blocking →
`policy::effective_expire_timeout` → `queue::push_with_outcome` → emite
`NotificationClosed`. Render: ticker da janela lê `snapshot`, `feed` diffa por
id, `geometry` recalcula, `popup` desenha no máximo 5. Dismiss: `window` →
`commands::request_dismissal` → `queue.request_close` → `host` drena e emite.
