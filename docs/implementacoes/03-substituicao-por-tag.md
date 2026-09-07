# 03 — Substituir notificações por tag

## Objetivo e escopo

Evitar uma pilha de avisos de volume/brilho quando o emissor não guarda o ID.
Não alterar layout nem implementar a barra da etapa 04.

## Contrato

Ler os hints string x-dunst-stack-tag e x-canonical-private-synchronous.
Uma tag válida tem de 1 a 128 caracteres; não aparar nem truncar a identidade.
Tipo incorreto, vazio ou comprimento excedido: ignorar o hint, sem falhar Notify.
Se ambos forem válidos, x-dunst-stack-tag tem precedência.

- Chave de correspondência: (Notice.app normalizado pelo fluxo atual, tag).
- Com replaces_id == 0 e tag válida, substituir somente uma notificação ATIVA
  com a mesma chave, preservando ID e atualizando conteúdo, timeout e chegada.
- Apps diferentes com a mesma tag nunca se substituem.
- replaces_id != 0 tem precedência, inclusive quando não existe:
  manter o comportamento atual de ID inexistente; não recorrer à tag.
- Sem correspondência ativa, criar normalmente. Não ressuscitar histórico.
- Na substituição, atualizar/mover o registro de histórico existente sem
  duplicá-lo, seguindo o comportamento atual de replaces_id.
- Se uma substituição explícita trocar/remover a tag, usar o valor novo.
- Caso substituições explícitas tenham criado várias notificações com a mesma
  chave, selecionar a mais recente da fila para substituição por tag.

## Implementação sugerida

Guardar tag opcional em Notice (ou representação mínima equivalente).
Ler hints na infraestrutura, mas resolver correspondência dentro da operação
de push da Queue, sob o mesmo lock da fila. Snapshot seguido de outro push
introduziria uma corrida. Reutilizar a lógica de substituição e IDs existente.
Atualizar construtores/testes afetados; não reescrever a fila ou criar cache.
Anunciar as duas extensões em GetCapabilities somente com suporte completo.

## Critérios de aceitação e testes

1. Mesma app/tag mantém ID e apenas um item ativo/histórico.
2. Apps distintas, tags distintas e ausência de tag criam itens distintos.
3. Cobrir precedência dos hints e replaces_id válido/inexistente.
4. Tag expirada/removida gera novo item, preservando o histórico anterior.
5. Testar tipo incorreto, vazio, Unicode e limite de tamanho.
6. Substituição explícita remove/troca tag sem deixar associação obsoleta.
7. Teste concorrente com duas inserções da mesma chave comprova uma única
   notificação ativa, sem impor qual conteúdo vence.
8. Exercitar Notify no D-Bus real de teste e conferir ID retornado, conteúdo e
   ausência de fechamento causado apenas pela substituição.

Referências: [hints do mako](https://github.com/emersion/mako/blob/master/dbus/xdg.c)
e [correspondência por aplicativo e tag](https://github.com/emersion/mako/blob/master/notification.c).

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
