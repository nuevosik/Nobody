# Design: Reorganização Clean Architecture + Clean Code — Nobody

Data: 2026-09-03
Status: aprovado pelo usuário (abordagem A)
Escopo: completo com docs/testes | Estrutura: manter mapa do `src/main.rs:4-7` | Liberdade: total interna, comportamento D-Bus externo inalterado

## 1. Contexto atual

Daemon `org.freedesktop.Notifications` em Rust (GPUI + LayerShell + zbus).

Flat atual em `src/`:

- `main.rs` (28 linhas): declara mapa limpo mas não refletido em pastas:
  `domain -> state, queue | application -> provider, time | infrastructure -> daemon, icons | presentation -> ui, theme`
- `daemon.rs` (285): interface D-Bus + `truncate`, `strip_markup:186-241`, `is_critical:161-177`, orquestra I/O de ícone
- `queue.rs` (376): `Queue` + `CloseReason:14-27` + `CloseRequest:29-33` + `PushOutcome:35-39` + `next_available_id:145-181` + `reserve_id:183-196`
- `state.rs` (68): `Notice:5-21` + `is_expired_at:18-20` + `Stack:64-68`
- `provider.rs` (51): `DEFAULT_EXPIRE_MS:6` + `effective_expire_timeout:9-17` + `snapshot/expire/request_dismissal:19-29`
- `icons.rs` (301): `resolve_notice_icon:14-41` + `icon_from_desktop:50-79` + `desktop_icon_key:81-97` + `CACHE:106-124` + `lookup_named_icon:126-200`
- `theme.rs` (64), `time.rs` (49), `ui/stack.rs` (495), `ui/popup.rs` (129), `ui/anim.rs` (105)
- `README.md:36-48` desatualizado (não lista `theme.rs`/`time.rs`)
- CI em `.github/workflows/ci.yml:11-14`: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`
- Violação central: `src/ui/stack.rs:13` faz `use crate::daemon` (presentation depende de infrastructure). Fere a Dependency Rule.

## 2. Arquitetura alvo

Regra: dependência só para dentro. `domain` não conhece ninguém. `application` só conhece `domain`. `infrastructure` e `presentation` conhecem para dentro, nunca entre si.

```
src/
  main.rs
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

`Stack { notices }` sai de `domain/state.rs` e vai para `presentation/shell/feed.rs` (estado de tela, não entidade). `time.rs` renomeia para `application/clock.rs`.

`main.rs` passa a ser só bootstrap:

```rust
let queue = domain::queue::Queue::new();
infrastructure::dbus::host::serve(queue.clone());
presentation::shell::window::open(queue.clone());
```

Nenhum `use crate::infrastructure` dentro de `presentation`. Nenhum `use crate::presentation` dentro de `infrastructure`. `mod.rs` de cada camada só re-exporta a API usada fora; resto `pub(crate)` ou privado.

## 3. Componentes (quebras SRP, teto ~150 linhas/arquivo)

| Origem | Destinos | Conteúdo |
|---|---|---|
| `daemon.rs` | `infra/dbus/daemon.rs` | struct + `Notify/CloseNotification/GetCapabilities/GetServerInformation` + sinais; delega todo o resto |
| | `infra/dbus/validation.rs` | `MAX_SUMMARY_LEN/BODY/ACTIONS/ACTION_LEN/HINTS/ICON_LEN`, `truncate`, `is_critical` |
| | `infra/dbus/markup.rs` | `strip_markup` + decodificação de entidades |
| `queue.rs` | `domain/queue.rs` | `Queue::push_with_outcome/remove/snapshot/remove_expired_at/request_close/drain_close_requests`, `KEEP=12` |
| | `domain/ids.rs` | `next_available_id` + `reserve_id` (wrap-around `u32::MAX` preservado) |
| | `domain/close.rs` | `CloseReason` (Expired=1, DismissedByUser=2, ClosedByCall=3, Undefined=4) + prioridades + `CloseRequest` + `PushOutcome` |
| `state.rs` | `domain/notice.rs` | `Notice` + `is_expired_at` (`expire_ms > 0` + `saturating_sub`) |
| `provider.rs` | `application/policy.rs` | `DEFAULT_EXPIRE_MS=5000` + `effective_expire_timeout` (crítico ou `0` nunca expira; `<0` usa default) |
| | `application/commands.rs` | `snapshot`, `expire`, `request_dismissal` (wrappers finos sobre `domain::queue`) |
| `time.rs` | `application/clock.rs` | `now_ms` (monotônico via `OnceLock<Instant>`), `elapsed_ms` (saturating) |
| `icons.rs` | `infra/icons/resolver.rs` | `resolve_notice_icon` (ordem: image-path, app_icon, desktop-entry, app) |
| | `infra/icons/lookup.rs` | `resolve_named_icon` + `lookup_named_icon` + ordem `SIZES` + roots XDG |
| | `infra/icons/desktop.rs` | `icon_from_desktop` (rejeita `/`, `..`, `>255`) + `desktop_icon_key` (limite 16KB, seção `[Desktop Entry]`) |
| | `infra/icons/cache.rs` | `CACHE: LazyLock<Mutex<HashMap>>` + `ICON_CACHE_LIMIT=256` com evicção de 1/4 |
| `ui/stack.rs` | `presentation/shell/window.rs` | `NotificationStack`, `new`, `dismiss`, `open_window`, `spawn_anim_ticker` |
| | `presentation/shell/geometry.rs` | `grouped_y_map` (O(n)), `grouped_y`, `total_h_current_for` (max, não último índice), `sync_window_geometry`, `DECK_GAP=16`, `MIN_HIT=24` |
| | `presentation/shell/feed.rs` | `Stack`, diff `snapshot` vs local, lista `Exiting`, retenção `< EXIT_MS` |
| | `infra/dbus/host.rs` | `serve` (connect session bus, `object_server().at`, `request_name DoNotQueue`, loop 100ms) + `flush_lifecycle_events` + `emit_notification_closed` |
| `ui/popup.rs`, `ui/anim.rs`, `theme.rs` | mesmos caminhos sob `presentation/` | só atualiza `use`; `badge_initial`, `badge` com fallback, `card_content`, `a11y_label`, `ENTER_MS=220`, `EXIT_MS=260`, `ease_out_cubic`, `prefers_reduced_motion` |

Funções seguem Clean Code: nome revela intenção, sem flag booleana oculta, sem comentário que repete o código, early return, `unwrap_or_else poison-safe` preservado nos `Mutex`.

## 4. Data flow

- `Notify`: `host/daemon` expira pendentes via `commands::expire` → `validation` (trunca app/summary/body/actions/hints) → `markup::strip_markup` → `icons::resolver` em `blocking::unblock` → `policy::effective_expire_timeout` → `queue::push_with_outcome` → emite `Closed(Expired/Undefined)` para expirados/evictados. `replaces_id != 0` e existente reinsere no topo com mesmo id; inexistente aloca novo e reserva sem squat.
- `Render`: ticker 16ms se `entering||exiting` senão 100ms; loop D-Bus 100ms chama `commands::snapshot`, `feed` diffa por id, move removidos para `exiting` com `y` agrupado, retém `< EXIT_MS`, `geometry` recalcula `total_h` e `input_region`, `popup` renderiza no máximo 5.
- `Dismiss` (click, Enter, Espaço, Escape): `window` → `commands::request_dismissal` → `queue.request_close(DismissedByUser)` → `host::flush` remove e emite `Closed` com precedência `DismissedByUser/ClosedByCall(3) > Expired(2) > Undefined(1)`, nunca rebaixa.

## 5. Errors

`domain`/`application` nunca logam nem fazem I/O; retornam `Option/Vec`. `infrastructure` concentra falhas: sem session bus e sem `PrimaryOwner/AlreadyOwner` mantém `eprintln!` atual + early return; `emit` mapeia para `fdo::Error::Failed`; `icons` retorna `None` em traversal, extensão não-imagem, diretório, `>512` chars, `.desktop >16KB`. `presentation::open_window` mantém `anyhow::Result`; `render` nunca panica.

Limites anti-DoS preservados: summary 200, body 500, actions 20 x 64, hints 64, icon 512, queue `KEEP=12`, close_requests `KEEP*2`, cache 256.

## 6. Testes

Testes unitários viajam com a função extraída sem mudar asserções (caps só `body`+`icon-static`, `strip_markup`, `truncate`, `is_critical`, `KEEP`, `replaces`, `expired`, precedência de `CloseReason`, `reserve(u32::MAX)` sem rewind, `ids` únicos, `badge_initial ß->S`, `app_font` fallback Inter, `elapsed_ms` saturating, `reduced_motion`).

Novos em `tests/`: `queue_invariants.rs` (cap 12, evict report, replaces fantasma não squatta), `dbus_contract.rs` (caps, `CloseNotification` silencioso para id inexistente). Guards no CI: `rg "infrastructure::|crate::(daemon|icons)" src/presentation` deve sair vazio; `rg "presentation::" src/infrastructure` idem.

## 7. Docs

- `docs/architecture.md`: diagrama da seção 2, Dependency Rule, tabela "onde pôr código novo" (regra nova de timeout → `application/policy`; novo formato de ícone → `infra/icons`; novo layout → `presentation/shell/geometry`).
- `README.md ## Arquitetura`: substituir árvore `36-48` pela árvore da seção 2 + 3 linhas de regra de dependência.
- Manter `clippy.toml`, `rustfmt.toml`, `Cargo.toml` e `ci.yml` inalterados.

## 8. Sucesso e não-escopo

Sucesso: `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` verdes; alvo `<=150` linhas por arquivo, teto rígido `<=200` linhas sem exceção; guards `rg` vazios; README e `docs/architecture.md` atualizados.

Não-escopo: nenhuma mudança no protocolo D-Bus externo; sem `ActionInvoked`, sem `image-data`, sem temas completos, sem config por urgência, sem histórico; sem troca de `gpui/zbus`; sem reestruturação por feature.
