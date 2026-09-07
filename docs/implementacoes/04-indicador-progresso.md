# 04 — Barra de progresso nos popups

## Objetivo e escopo

Exibir o hint value como barra discreta no cartão existente.
Pode funcionar sozinho; a etapa 03 permite atualizar o mesmo popup por tag.

## Contrato

- Ler value de tipo D-Bus int32; aceitar 0 a 100 inclusive.
- Ausência, tipo incorreto ou valor fora do intervalo: nenhuma barra.
  Não confundir zero com ausência, nem converter strings.
- Guardar como Option<u8> em Notice, preservado no histórico.
- Atualização do mesmo ID atualiza a barra. Substituição sem value remove a barra.
- Manter dimensões externas atuais do cartão e regiões de clique coerentes.
- Mostrar o percentual em texto/descrição acessível, além da diferença de cor.
- A barra não captura foco nem clique; teclado e dispensa continuam iguais.
- Respeitar DND/tela cheia. Não produzir animação contínua nem replay de histórico.
- Não é contagem regressiva do timeout. Não adicionar sons ou configurações.

## Implementação sugerida

Parsing em infrastructure/dbus/validation.rs/daemon.rs; dado no domínio;
render no popup existente, respeitando o espaço interno. Usar primitivas GPUI
já instaladas, sem biblioteca de gráficos. Verificar feed::sync_snapshot:
mudança apenas no progresso deve invalidar a apresentação.
Nesta etapa basta desenhar a barra nos popups, sem redesenhar a central.
Se a etapa 01 existir, acrescentar progress (inteiro ou null) aos itens JSON.

## Critérios de aceitação e testes

1. Hints ausentes, 0, 50, 100, -1, 101 e tipo incorreto.
2. Mesma notificação em 10 → 90 → sem hint: mesmo ID, atualização detectada,
   e progress final None.
3. Dados do histórico e da listagem, se disponível, correspondem ao recebido.
4. Entrada por Notify no barramento isolado chega intacta ao domínio.
5. Validar visualmente barra vazia/meia/cheia, texto longo e cartões agrupados:
   sem sobreposição de conteúdo, alteração de altura ou área clicável perdida.
6. Verificar que a descrição acessível inclui o percentual quando presente.

Referência: [value e progress-color no mako](https://github.com/emersion/mako/blob/master/doc/mako.5.scd).

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
