# Nobody

A Wayland notification daemon. It owns `org.freedesktop.Notifications` on the
session bus and renders notifications top-right in a layer-shell window.

Built with [GPUI](https://github.com/zed-industries/zed) and zbus. No GTK, no
`notify-osd` fork.

## Preview

Spotify notification with the current album cover:

![Spotify notification with album cover](docs/spotify-cover.png)

Notification center with search, Do Not Disturb and Waybar integration:

![Nobody notification center with an empty inbox](docs/notification-center.png)

## Install

```sh
curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh
```

That drops `nobody` in `~/.local/bin`. Override the destination with
`NOBODY_INSTALL_DIR`.

### Compositor

Stop any daemon already owning the name (e.g. mako), start nobody:

```conf
exec-once = pkill mako; nobody
```

Make sure `~/.local/bin` is on `PATH` for the compositor.

### From source

```sh
git clone https://github.com/nuevosik/Nobody
cd Nobody
cargo run --release
```

Needs a Wayland session with Layer Shell, a D-Bus session bus, Rust, and the
usual GPUI/Linux packages (`libwayland`, `libxkbcommon`, Vulkan).

## Use

- Click or Escape dismisses a notification. Enter and Space dismiss it too, unless
  an action button is focused, in which case they activate that action.
- Keeps 12 notifications, renders the 5 most recent.
- Timeout: `-1` means server default (5s), `0` never expires; critical
  notifications never auto-expire.
- O daemon anuncia as capabilities `body` e `actions`. Cada par
  chave/rótulo recebido (até 10 ações, 20 itens chave/rótulo; chaves vazias
  ou repetidas são ignoradas e rótulos são cortados em 64 caracteres) gera um
  botão no popup ativo: `default` mostra
  `Abrir`, as demais chaves mostram o rótulo recebido (ou `Ação` se vazio).
  Clique, Enter ou Space no botão ativa a ação;
  a chave original é devolvida opaca via `ActionInvoked` e o popup fecha com
  razão 2, preservando o histórico. Chaves desconhecidas, itens expirados,
  removidos ou com fechamento pendente não geram ação. A central é histórico
  de leitura: não oferece ações nem reproduz itens antigos. O daemon nunca
  executa a chave como comando, URL ou argumento de shell: o cliente decide
  a navegação. No Hyprland, a ação `default` também foca a janela mapeada mais
  recente do `desktop-entry` (ou do nome do aplicativo), mudando de workspace
  quando necessário. Outros compositores ainda podem exigir token de ativação.
- Notify aceita `x-dunst-stack-tag` e `x-canonical-private-synchronous` como
  tags string de 1 a 128 caracteres; a primeira tem precedência quando ambas
  são válidas. Na mesma aplicação, uma tag ativa substitui a notificação
  anterior, preservando seu ID e atualizando conteúdo, timeout e chegada.
- Notify aceita o hint `value` somente como inteiro D-Bus `int32` entre 0 e 100;
  quando válido, exibe uma barra discreta no popup e inclui o percentual na
  descrição acessível. `list --json` e `history --json` exportam `progress` como
  inteiro ou `null`.
- Spotify notifications show the current album cover (via MPRIS + `curl`),
  cached under `~/.cache/nobody/covers/`.

### Central e Não Perturbe

```sh
nobody
nobody center open | close | toggle
nobody dismiss all
nobody dismiss 42
nobody dismiss app "Spotify"
nobody dnd on | off | toggle
nobody dnd status
nobody list --json
nobody history --json
```

- `nobody list --json` e `nobody history --json` exportam snapshots da fila ativa e do histórico
  em JSON em linha única no formato `{"notifications":[...]}` (com `seq` no histórico).

- A configuração opcional fica em `$XDG_CONFIG_HOME/nobody/config` quando
  `XDG_CONFIG_HOME` é absoluto; caso contrário, em `$HOME/.config/nobody/config`.
  Os defaults são `default-timeout=5000`, `max-visible=5` e `anchor=top-right`.
  O arquivo aceita essas três chaves, comentários em linhas próprias e valores
  repetidos com o último valor válido; alterações exigem reiniciar o daemon.

- Os comandos de central permitem abrir (`nobody center open`), fechar (`nobody center close`)
  ou alternar (`nobody center toggle`) a central de forma idempotente e previsível;
  todos preservam notificações ativas, histórico e preferências de silêncio.
- A central lista as últimas 100 notificações da sessão (mais recentes
  primeiro), com busca por aplicativo, título ou corpo, botão de limpar e
  controle de Não Perturbe. Escape fecha; expirar ou dispensar um popup
  preserva o registro; limpar preserva a fila ativa.
- Silêncio efetivo = manual OU tela cheia. O detector de tela cheia nunca
  sobrescreve a preferência manual; ao sair do silêncio, só aparecem
  notificações ainda ativas (o histórico não é reproduzido).
- `nobody dismiss all` dispensa todas as notificações ativas de uma vez,
  preservando o histórico. Pode ser associado a um atalho do desktop.
- `nobody dismiss 42` dispensa uma notificação pelo ID decimal mostrado na
  listagem; `nobody dismiss app "Spotify"` dispensa todas as notificações cujo
  aplicativo seja exatamente o nome mostrado na listagem (maiúsculas e espaços
  fazem parte da comparação). IDs inexistentes e aplicativos sem correspondência
  terminam com sucesso sem efeito. A seleção por aplicativo usa o snapshot do
  momento da chamada; notificações posteriores ficam para uma próxima operação.
- Os comandos de controle retornam 0 quando concluídos, 1 quando o daemon/D-Bus
  falha e 2 para sintaxe inválida; esses comandos não iniciam a GUI. Dispensar
  preserva o histórico, o estado da central e Não Perturbe.
- Limitação desta entrega: histórico apenas em memória — sem banco ou
  persistência. A exportação JSON mostra somente os snapshots atuais e do
  histórico mantido durante a sessão; não há abertura automática de
  aplicativos. Reiniciar o daemon começa com histórico vazio.

### Brave / YouTube

O Nobody recebe o que o navegador entrega ao sistema via
`org.freedesktop.Notifications`. Ele não lê o sino interno do YouTube, não
recupera alertas antigos da conta e não substitui as permissões/push do
navegador. O Chromium usa o backend freedesktop quando o serviço anuncia
`body` e `actions`; uma instalação isolada (Flatpak/Snap) pode usar o
portal em vez do bus direto.

Para validar a integração:

1. Confira que `GetCapabilities` anuncia `body` e `actions`.
2. No Brave, permita notificações e use a [página oficial de teste
   push](https://support.brave.com/hc/en-us/articles/360058972091-Push-Notification-Test).
   `notify-send` sozinho não valida o navegador.
3. No YouTube, ative a opção de receber notificações neste navegador e as
   preferências da conta/canais conforme a [ajuda
   oficial](https://support.google.com/youtube/answer/9334950?hl=en); aguarde
   uma notificação legítima. O teste push genérico não prova que a conta
   está configurada.
4. Se nada chegar, verifique as permissões do site e a opção de serviços
   Google para push na versão instalada. O Brave pode precisar reiniciar
   para renegociar capabilities; salve seu trabalho antes.
5. Clicar em Abrir devolve a chave ao navegador via `ActionInvoked`; no
   Hyprland, o Nobody também segue para o workspace da janela correspondente.

### Waybar

With Nobody installed in `~/.local/bin`, run from this checkout:

```sh
install -Dm644 assets/nobody-badge.png "$HOME/.local/share/nobody/nobody-badge.png"
```

Add `"image#nobody"` to `modules-right` in your Waybar configuration, then
add this module at the top level:

```json
"image#nobody": {
    "exec": "printf '%s\\n' \"$HOME/.local/share/nobody/nobody-badge.png\" 'Central de notificações'",
    "size": 16,
    "interval": 3600,
    "on-click": "\"$HOME/.local/bin/nobody\" center toggle",
    "tooltip": true
}
```

Add to Waybar's `style.css`:

```css
#image.nobody { padding: 0 8px; }
#image.nobody:hover { background: rgba(255, 255, 255, 0.10); }
```

Restart Waybar to load the module. Clicking the icon toggles the central.
If Nobody was installed elsewhere, adjust `on-click` to that binary.
This setup is optional; the installer does not modify your Waybar configuration.

Debug:

| env | effect |
| --- | --- |
| `PREFERS_REDUCED_MOTION=1` | disables animations |

## Behavior

- `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`;
  emits `NotificationClosed` and `ActionInvoked`.
- `replaces_id` replaces atomically.
- `x-dunst-stack-tag` e `x-canonical-private-synchronous` substituem uma
  notificação ativa da mesma aplicação quando `replaces_id` é zero; tags
  inválidas são ignoradas e substituições explícitas por ID têm precedência.
- Icon path/name plus `desktop-entry` lookup scoped to known locations;
  markup is stripped. No `image-data`, persistence, body-markup, inline
  replies or activation tokens; cross-workspace focus uses Hyprland IPC.

## Architecture

Four layers, one rule: `presentation` and `infrastructure` never import each
other — they talk through the `domain` queue. Details in
`docs/architecture.md`.

```
domain <- application <- infrastructure / presentation
```

## Development

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Release

```sh
./scripts/release.sh 0.1.1
```

Bumps `Cargo.toml`, tags `v0.1.1`, pushes. GitHub Actions builds
`nobody-<triple>.tar.gz` and attaches it to the release, which is what
`install.sh` downloads.
