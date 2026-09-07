# 05 — Executar a ação padrão da notificação

## Objetivo e escopo

Adicionar um botão “Abrir” somente quando a notificação ativa anunciar uma
ação com chave default. Não implementar menu de ações, comandos shell ou
ações em registros históricos. Independente das etapas 01–04.

## Contrato

- actions recebido por Notify é uma sequência de pares chave/rótulo.
  Validar pares completos; ignorar chave vazia e par final incompleto.
- Preservar limites existentes sem truncar uma chave e emitir uma identidade
  diferente: descartar pares cuja chave exceda o limite. Rótulos podem ser
  truncados. Para chaves duplicadas, manter o primeiro par válido.
- O botão não deve disparar o handler de dispensa do cartão por propagação.
  Deve ter foco, nome acessível e ativação por Enter/Space.
- Ao ativar, revalidar no daemon que o ID ainda existe e ainda oferece default.
  Notificação já removida/expirada ou ação removida: nenhum sinal de ação.
- Emitir ActionInvoked(id, "default") antes de NotificationClosed(id, 2).
  Uma ativação efetiva fecha o popup e mantém histórico.
- Cliques repetidos antes do próximo ciclo não podem emitir duas ações.
- Falha ao emitir ActionInvoked: registrar erro e preservar a notificação;
  não fechar como se tivesse funcionado nem repetir automaticamente a ação.
- Não executar título, corpo, chave ou rótulo como comando/URL local.
- Não anunciar a capability actions nesta entrega limitada à ação default:
  isso sugeriria suporte às ações nomeadas, que ficam para outra tarefa.
  Documentar que alguns clientes podem não enviar ações por esse motivo.
- Não prometer foco/abertura do aplicativo: sem token de ativação Wayland,
  essa decisão depende do cliente e compositor. Token fica fora desta entrega.

## Implementação sugerida

Notice.actions e a declaração de ActionInvoked já existem, mas não completam
o fluxo. Criar o menor pedido de ação compartilhado necessário no domínio,
com deduplicação e limite; a UI passa pela aplicação e o host processa/emite
D-Bus. Nunca importar infraestrutura na apresentação.

Antes de editar, rastrear close requests, expiração e substituição. Processar
pedidos de ação com identidade revalidada e ordem definida; se o mesmo ID
também tem pedido de fechamento, não executar ação depois de fechado.
Não criar um segundo estado persistente de notificações.

## Critérios de aceitação e testes

1. Parsing de pares, quantidade/tamanho máximos, chave duplicada e ímpar.
2. Botão apenas para default; uma notificação sem ação mantém o comportamento.
3. Barramento isolado: capturar ActionInvoked e NotificationClosed e conferir
   chave, razão, ordem e emissão única.
4. ID removido/expirado, default removido por substituição e clique repetido.
5. Erro de emissão não remove o item nem o repete automaticamente.
6. Histórico preservado e nenhum comando do sistema executado.
7. Validar visualmente clique sem propagação, foco e teclado. Se não puder,
   declarar a lacuna; teste de domínio não comprova interação gráfica.

Referência: [try_invoke_action no mako](https://github.com/emersion/mako/blob/master/notification.c).
Esta entrega é deliberadamente menor que o suporte completo do mako.

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
