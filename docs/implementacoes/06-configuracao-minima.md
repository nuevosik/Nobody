# 06 — Configuração global mínima

## Objetivo e escopo

Permitir três ajustes no início do daemon: timeout padrão, quantidade visível
e canto superior. Independente das etapas anteriores; preserve-as se aplicadas.

Arquivo: $XDG_CONFIG_HOME/nobody/config quando XDG_CONFIG_HOME for absoluto e
não vazio; caso contrário, $HOME/.config/nobody/config. Se nenhum caminho puder
ser determinado, usar defaults. Nunca sobrescrever variáveis HOME do ambiente.

```ini
default-timeout=5000
max-visible=5
anchor=top-right
```

## Contrato

| Chave | Valores aceitos | Default |
| --- | --- | --- |
| default-timeout | inteiro decimal de 0 a 86400000 ms | 5000 |
| max-visible | inteiro de 1 a KEEP (atualmente 12) | 5 |
| anchor | top-right ou top-left | top-right |

- Sintaxe key=value, espaços nas extremidades ignorados, linhas vazias e
  comentários de linha inteira começando por #. Sem comentários inline.
- Chave repetida: último valor válido vence. Chave desconhecida ou valor
  inválido torna o arquivo inteiro inválido; não aplicar parcialmente.
- Arquivo ausente: defaults silenciosos. Falha de leitura, UTF-8 inválido,
  arquivo maior que 64 KiB ou sintaxe inválida: uma mensagem clara em stderr
  e defaults completos, sem panic. Limitar a leitura, não apenas consultar tamanho.
- Ler somente no startup do daemon. Comandos de controle não leem o arquivo.
- Timeout configurado vale somente para timeout solicitado negativo.
  Zero explícito e urgência crítica continuam sem expiração; positivo explícito
  do aplicativo continua respeitado.
- max-visible controla slots de apresentação, preservando a lógica de grupos.
  Não alterar KEEP, capacidade de histórico ou descartar itens adicionais.
- top-left muda a ancoragem da superfície; popups e central ficam no canto
  escolhido. Não inverter texto ou ordem interna do cartão.
- Defaults mantêm comportamento e visual atuais.

## Implementação sugerida

Tipo de configuração simples com Default; parsing puro testável e leitura na
infraestrutura. Passar valores imutáveis necessários no bootstrap para política
de timeout e apresentação. Sem singleton mutável, parser de terceiros ou DI
genérica. O parser deste formato curto pode usar stdlib.

Inspecionar policy::effective_expire_timeout, visible_count, cálculo de decks,
geometry e open_window. Não basta mudar uma constante se outro cálculo ainda
assume cinco slots. Reutilizar Anchor do LayerShell existente.

## Critérios de aceitação e testes

1. Defaults, arquivo válido, espaços, comentários, duplicatas e limites.
2. Chaves desconhecidas, negativo, overflow, valor vazio, linha sem = e
   anchor inválido rejeitam atomicamente a configuração.
3. Leitura limitada e erros de arquivo; testar caminhos com diretórios
   temporários, sem editar a configuração real do usuário.
4. Timeout negativo usa valor configurado; zero/positivo/crítico preservados.
5. Layout com 1, 5 e 12 slots, incluindo grupos recolhidos/expandidos.
6. Verificar os dois anchors e central aberta em sessão gráfica de teste.
7. README documenta caminho, exemplo, defaults e necessidade de reiniciar.

Fora do escopo: reload, monitor/output, posição inferior, estilos/cores/fontes,
includes, regex e regras por aplicativo. Essas extensões exigem tarefas próprias.

Referência: [configuração do mako](https://github.com/emersion/mako/blob/master/doc/mako.5.scd).

## Instruções de execução e entrega

Implemente somente este documento. Leia AGENTS.md e o código atual primeiro:
as etapas anteriores podem já estar aplicadas. Preserve alterações preexistentes.
Reutilize helpers; mantenha domain ← application ← infrastructure/presentation,
sem imports entre infraestrutura e apresentação. Não faça commit ou push.

Amplie os testes existentes, sem framework novo. Consulte Context7 para APIs
de bibliotecas que precisar alterar e confira a versão instalada. Atualize o
README com o comportamento entregue. Execute:

```sh
rtk proxy cargo fmt --all -- --check
rtk cargo check --all-targets
rtk cargo clippy --all-targets -- -D warnings
rtk cargo test --all-targets
rtk proxy git diff HEAD --check
```

Para testes D-Bus, use servidor de teste e barramento isolado; nunca o daemon
real. Se disponível, execute também:

```sh
rtk proxy dbus-run-session -- cargo test --all-targets -- --nocapture
```

Não apresente teste pulado como integração validada. Entregue arquivos alterados,
comandos/resultados reais e limitações. Para mudanças visuais, valide em sessão
gráfica de teste ou declare explicitamente que a GUI não foi verificada.
Outra LLM revisará o diff e repetirá os checks antes da próxima etapa.
