# 01 — Listar notificações e histórico em JSON

## Objetivo e escopo

Expor os snapshots existentes por CLI, sem GUI, persistência ou nova fila.
Esta é a primeira etapa; não depende das outras cinco.

```sh
nobody list --json
nobody history --json
```

Nesta entrega, exigir exatamente esses argumentos. Não implementar tabela de
texto, filtros, paginação, watch ou limpeza de histórico.

## Contrato

- Sucesso: código 0 e um objeto JSON em uma linha, seguido de newline.
- Formato: {"notifications":[...]}; fila vazia: {"notifications":[]}.
- Cada item: id (número), app, summary e body (strings), expire_ms (número).
  Histórico inclui também seq (número), para distinguir registros.
- Preservar a ordem do snapshot: mais recentes primeiro. Não ordenar por ID.
- O histórico inclui registros ainda ativos, conforme o modelo atual do Nobody;
  não copiar a semântica de “apenas dispensadas” do mako.
- Não exportar arrived_at_ms como horário civil: é um relógio interno.
- Leituras não removem, expiram ou modificam dados, nem abrem a central.
- Falha D-Bus: código 1 e stderr, sem JSON parcial no stdout.
- Argumentos ausentes/desconhecidos/extras: código 2 e uso.

## Implementação sugerida

Parser em src/application/cli.rs, dispatch em src/main.rs e métodos List/History
na interface com.nobody.Control. Reutilizar commands::snapshot e commands::history.
Para manter o contrato pequeno, métodos D-Bus podem devolver uma string JSON;
a serialização fica na infraestrutura, usando serde_json já instalado.
Não adicionar serde ao domínio nem dependências novas apenas para derives.
Usar serializador real; nunca concatenar strings JSON manualmente.

## Critérios de aceitação e testes

1. Parser aceita ambos e rejeita variantes fora do escopo.
2. Popular fila e histórico, remover um item ativo e conferir a diferença entre
   os dois resultados, incluindo ordem e seq.
3. Testar vazio, aspas, barras, newline e Unicode; validar saída com serde_json.
4. Comparar estado antes/depois, inclusive pedidos de fechamento e DND.
5. Chamar List e History no servidor D-Bus de teste e validar os documentos.
6. Testar binário com argumentos inválidos e sem daemon em barramento isolado,
   conferindo códigos e stdout/stderr.

Referência de comportamento: [makoctl list/history](https://github.com/emersion/mako/blob/master/doc/makoctl.1.scd).

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

