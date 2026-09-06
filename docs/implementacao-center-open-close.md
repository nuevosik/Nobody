# Tarefa: abrir e fechar a central explicitamente

Implemente esta tarefa no repositório Nobody. Leia as instruções locais e o
código existente antes de editar. Faça a menor alteração que satisfaça os
critérios abaixo. Outra LLM fará a revisão do diff e executará as verificações.

## Objetivo

Adicionar dois comandos para scripts e atalhos que precisam de um estado
previsível, sem depender do estado anterior da central:

```sh
nobody center open
nobody center close
```

O comando existente `nobody center toggle` continua funcionando.

## Comportamento obrigatório

| Situação | Resultado esperado |
| --- | --- |
| Central fechada; executar `open` | Abre a central |
| Central aberta; executar `open` novamente | Continua aberta |
| Central aberta; executar `close` | Fecha a central |
| Central fechada; executar `close` novamente | Continua fechada |
| Silêncio automático por tela cheia ativo; executar `open` | Continua fechada, sem erro |
| Sair da tela cheia após esse `open` | Continua fechada; não agendar abertura |
| Apenas Não Perturbe manual ativo; executar `open` | Abre a central, preservando Não Perturbe |

Ambos os comandos devem preservar notificações ativas, histórico, pedidos de
fechamento e as preferências de silêncio. Fechar a central não dispensa popups.

Sucesso retorna código 0, sem saída obrigatória. Falha de conexão/chamada ao
daemon retorna código 1 e mensagem em stderr, seguindo o tratamento existente.
Argumentos inválidos ou extras retornam código 2 e a mensagem de uso.
Os comandos não devem inicializar a GUI nem iniciar um novo daemon.

## Caminho de implementação

1. Em `src/application/cli.rs`, adicionar variantes `CenterOpen` e
   `CenterClose`, reconhecer os argumentos exatos e atualizar `USAGE`.
2. Em `src/infrastructure/dbus/control.rs`, expor os métodos sem argumentos
   `center_open` e `center_close` em `ControlService`. Reutilizar
   `commands::set_center_open(&self.queue, true/false)`, já existente.
   Adicionar as declarações correspondentes ao proxy e funções cliente
   bloqueantes, seguindo `center_toggle`, `blocking_proxy` e `call_err`.
   Os métodos D-Bus esperados são `CenterOpen` e `CenterClose`, na interface
   `com.nobody.Control`, caminho `/com/nobody/Control`, serviço
   `org.freedesktop.Notifications`.
3. Em `src/main.rs`, encaminhar as novas variantes em `run_control`.
4. Atualizar os testes existentes e o exemplo de comandos no `README.md`,
   explicando brevemente a diferença entre `open`, `close` e `toggle`.

O domínio já oferece `Queue::set_center_open` com a proteção de tela cheia.
A apresentação já observa esse estado. Não é necessário criar outro estado,
alterar a GUI ou duplicar a proteção no adaptador.

## Testes necessários

Amplie os testes existentes; não adicione framework ou infraestrutura nova.

- Parser: aceitar os dois comandos e rejeitar `center`, `center invalid`,
  `center open extra` e `center close extra`. Atualizar o teste que atualmente
  considera `center open` inválido.
- Serviço sem barramento: verificar a sequência `open`, `open`, `close`,
  `close`, com asserts após cada chamada. Popular a fila/histórico e confirmar
  que permanecem iguais e que nenhum pedido de fechamento foi criado.
- Serviço sem barramento: verificar os casos de tela cheia e Não Perturbe
  manual da tabela, incluindo a saída da tela cheia sem abertura automática.
- Integração em `tests/control_cli.rs`: ampliar o daemon de teste com nome
  próprio para chamar `CenterOpen` e `CenterClose` pelo D-Bus e conferir o estado
  compartilhado. Não controlar o daemon real durante os testes automatizados.

## Verificação e entrega

Execute os checks usados pelo projeto, respeitando o prefixo `rtk` das
instruções locais:

```sh
rtk proxy cargo fmt --all -- --check
rtk cargo check --all-targets
rtk cargo clippy --all-targets -- -D warnings
rtk cargo test --all-targets
rtk proxy git diff --check
```

Se o teste de D-Bus pular por falta de barramento, declare isso no relatório;
um teste pulado não comprova a integração. Se disponível, use um barramento
isolado para executar o teste existente:

```sh
rtk proxy dbus-run-session -- cargo test --test control_cli -- --nocapture
```

Com uma sessão gráfica de teste disponível, verificar também que a central
abre/fecha visualmente e que repetir cada comando mantém o estado. Caso não
seja possível, informar que a GUI não foi validada.

Entregue o diff e um resumo contendo arquivos alterados, comandos executados,
resultados reais e limitações. Não faça commit ou push nesta tarefa.

## Limites do escopo

Sem dependências novas, persistência, opções de configuração, mudanças de
layout, refatorações gerais ou alterações em `.codex/` e `AGENTS.md`.
Preserve mudanças preexistentes de outras pessoas.

A revisão posterior verificará o caminho completo CLI → cliente D-Bus →
serviço → estado compartilhado, a idempotência, o respeito à tela cheia,
a preservação dos dados e a execução dos testes. Apenas reconhecer os novos
argumentos, sem conectar o serviço, não conclui a tarefa.
