# Fluxo de Testes Locais — Contrato Soroban (com pausa/retomada)

> Este guia mostra **como testar tudo local**, em duas camadas:
> 1) **Testes em Rust** (sem rede) usando `soroban-sdk` + `testutils`.
> 2) **Rede local Quickstart** (Docker) com **stellar-core + RPC + Horizon + friendbot** para exercitar **CLI → deploy → invoke → eventos**.
>
> Baseado **exclusivamente** na documentação oficial Stellar Developers:
> - Soroban (contratos): https://developers.stellar.org/docs/build/smart-contracts
> - CLI (stellar-cli): https://developers.stellar.org/docs/build/smart-contracts/getting-started/cli
> - Ambiente / Ledger timestamp: https://developers.stellar.org/docs/build/smart-contracts/environment#ledger
> - Storage: https://developers.stellar.org/docs/build/smart-contracts/storage
> - Auth: https://developers.stellar.org/docs/build/smart-contracts/auth
> - Events + getEvents: https://developers.stellar.org/docs/build/smart-contracts/events
> - Stellar Asset Contract (SAC) / Token Interface: https://developers.stellar.org/docs/learn/stellar-asset-contract

---

## Pré‑requisitos

- **Rust** + target wasm:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Docker** (para a rede local Quickstart)
- **Stellar CLI** (uma opção):
  ```bash
  # macOS (Homebrew)
  brew install stellar-cli

  # Ou multiplataforma via cargo
  cargo install --locked stellar-cli
  ```

---

## Estrutura mínima do projeto

```
access-time/
├─ Cargo.toml
├─ src/
│  └─ lib.rs          # seu contrato Soroban (com buy/start/pause/remaining/...)
└─ tests/
   └─ contract.rs     # testes locais (sem rede) usando soroban-sdk testutils
```

**Cargo.toml** (trecho relevante):
```toml
[dependencies]
soroban-sdk = "22"

[dev-dependencies]
soroban-sdk = { version = "22", features = ["testutils"] }
```

---

# A) Testes em Rust (sem rede)

### 1) O que você consegue testar aqui
- Regras de **autorização** (`require_auth`).
- Lógica de **saldo de segundos** com **start/pause/buy**.
- **Eventos** sendo publicados.
- Um **SAC de teste** para simular `mint/balance/transfer` (sem rede real).

### 2) Exemplo de teste (`tests/contract.rs`)

```rust
#![cfg(test)]
use soroban_sdk::{
  testutils::{Address as _, Ledger},
  token::{Client as TokenClient, StellarAssetClient},
  Address, Env,
};
use access_time::{AccessTime, AccessTimeClient, Session};

#[test]
fn buy_start_pause_flow() {
  let env = Env::default();
  env.mock_all_auths(); // autorizações "ok" nos testes

  let admin = Address::generate(&env);
  let user  = Address::generate(&env);

  // SAC de teste (token compatível)
  let sac = env.register_stellar_asset_contract_v2(admin.clone());
  let token_id = sac.contract_id();

  // Registra o contrato e cria client
  let cid = env.register(AccessTime, ());
  let client = AccessTimeClient::new(&env, &cid);

  // init + pacote
  client.init(&admin, &token_id);
  client.set_package(&1u32, &10_i128, &3600u32);

  // Mint para o user
  let token_admin = StellarAssetClient::new(&env, &token_id);
  token_admin.mint(&user, &100_i128);

  // Compra
  client.buy(&user, &1u32);
  let mut s: Session = client.get_session(&user);
  assert_eq!(s.started_at, 0);
  assert_eq!(s.remaining_secs, 3600);

  // Start
  client.start(&user);
  s = client.get_session(&user);
  assert!(s.started_at > 0);

  // Avança 120s no "reloginho" do ledger (ambiente de teste)
  let t0 = env.ledger().timestamp();
  env.ledger().with_mut(|li| { li.timestamp = t0 + 120; });

  // Remaining reduziu ~120
  let now = env.ledger().timestamp();
  let remaining = client.remaining(&user, &now);
  assert!(remaining <= 3600 - 120);

  // Pause: congela
  client.pause(&user);
  s = client.get_session(&user);
  assert_eq!(s.started_at, 0);
  let frozen = s.remaining_secs;

  // Avança mais 600s: saldo permanece
  env.ledger().with_mut(|li| { li.timestamp = now + 600; });
  let still = client.remaining(&user, &(now + 600));
  assert_eq!(still, frozen);
}
```

### 3) Rodar
```bash
cargo test
```

> Vantagens: extremamente rápido, determinístico e sem rede. Ideal para TDD e cobrir 90% da lógica.

---

# B) Rede Local Quickstart (Docker)

### 1) Subir a rede local
```bash
stellar container start local
```
Isso inicia um container com **core + RPC + Horizon + friendbot**.

**Endpoints padrão (local):**
- RPC: `http://localhost:8000/rpc`
- Horizon: `http://localhost:8000`
- Passphrase: `Standalone Network ; February 2017`

### 2) Configurar a rede e chaves na CLI
```bash
stellar network add local \
  --rpc-url http://localhost:8000/rpc \
  --network-passphrase "Standalone Network ; February 2017"
stellar network use local

```

- `stellar network add local ...` → cria um apelido chamado `local` na CLI, apontando para o RPC e passphrase da rede que você subiu.
- `--rpc-url` → o endereço do RPC rodando no container `(http://localhost:8000/rpc)`.
- `--network-passphrase` → frase de rede obrigatória que identifica essa rede. Para o Quickstart é sempre `"Standalone Network ; February 2017"`.
- `stellar network use local` → define essa rede como default para os próximos comandos (contract deploy, invoke, events, etc.).

Pensa nisso como configurar o `kubectl config use-context` no Kubernetes: você diz “agora quero mandar meus comandos para essa rede aqui”.

#### 2.1) Criar e financiar contas de teste

```bash
stellar keys generate admin
stellar keys generate alice
```

- Gera pares de chaves (pública/privada) e salva num storage interno da CLI (`~/.stellar/keys.json`).
- Aqui criamos 2 contas: `admin` (vai gerenciar pacotes/deploys) e `alice` (usuário que compra internet).
- Você pode listar com `stellar keys list`.


```bash
stellar keys fund admin
stellar keys fund alice
```

- Usa o **friendbot local** para criar a conta on-chain e depositar fundos iniciais (XLM “de mentira”) -- Add os 10k em XML para brincar kk.
- Sem isso, a conta não existe no ledger e não consegue pagar fees.
- Depois disso, `admin` e `alice` já têm XLM local para assinar/deployar/invocar contratos.

```bash
ADMIN_G=$(stellar keys public-key admin)
ALICE_G=$(stellar keys public-key alice)
```

- `stellar keys public-key <nome>` → mostra a **chave pública (endereço G...)**.
- Guardamos em variáveis de shell (`$ADMIN_G, $ALICE_G`) porque esses valores são usados depois em comandos como `init` ou `buy`.
- Exemplo de saída:

```bash
GCFI4VQ4Z2Y... (admin)
GA62W4TSY3L... (alice)
```

#### 2.2 Consultar balace carteiras criadas

- Via Horizon Local:
```bash
curl -s "http://localhost:8000/accounts/$(stellar keys public-key alice)" | jq .balances

```
- resultado:
```json
[
  {
    "balance": "10000.0000000",
    "buying_liabilities": "0.0000000",
    "selling_liabilities": "0.0000000",
    "asset_type": "native"
  }
]
```

#### 2.2.1 - Pegar chaves publicas e salvar em variavel local -- via sac

```bash
ADMIN_G=$(stellar keys public-key admin)
ALICE_G=$(stellar keys public-key alice)
echo $ADMIN_G
echo $ALICE_G
```
#### 2.2.2 - Deploy do contrato do asset nativo (XLM)

```bash
<# Esse comando implanta o Stellar Asset Contract (SAC) para o XLM nativo no ledger local: #>
stellar contract asset deploy --asset native --source-account admin
```
- `--asset native` → indica que é o nativo (XLM).
- `--source-account admin` → a identidade `admin` (que você já criou/fundou) vai assinar a transação.

Se o contrato já existir, pode aparecer erro “already deployed” — nesse caso, tudo bem, é só pular para o próximo passo.

#### 2.2.3 - Recuperar o contractId do SAC
```bash
TOKEN_XLM_ID=$(stellar contract id asset --asset native)
echo $TOKEN_XLM_ID
```
Isso deve te devolver algo no formato `CDMLF...G4EI64` (o endereço do contrato).

#### 2.2.4 - Consultar saldo da Alice (via SAC do nativo)
```bash
ALICE_G=$(stellar keys public-key alice)

stellar contract invoke --id $TOKEN_XLM_ID -- balance --id $ALICE_G

```
- Isso retorna o saldo em **XLM** que a Alice tem na rede local (depois que você rodou stellar keys fund alice).
- Com isso, o SAC do nativo (XLM) fica ativo na sua rede local e você consegue usar métodos como balance e transfer.

### 3) Build e deploy do contrato
- Pelo que percebi a cli da stellar está usando por padrão o target `wasm32v1-none`. Então será necessário add com `rustup`
- use o `cargo clean` para limpar a pasta `/target`

```bash
# build wasm -- stay-oline(Nome do projeto no cargo.toml) - no meu caso stay-online
stellar contract build --package stay-online

# Otimizar o WASM com soroban-cli
stellar contract optimize --wasm target/wasm32v1-none/release/stay_online.wasm

# deploy (guarde o Contract ID) pode usar stellar -v e rode em modo verboso sem otimized
CONTRACT_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/stay_online.wasm --source-account admin | tail -n 1)

#deploy otimizado
CONTRACT_ID_OPTIMIZED=$(stellar contract deploy --wasm target/wasm32v1-none/release/stay_online.optimized.wasm --source-account admin | tail -n 1)

```

### 4) Inicializar contrato/ testar contrato

Nesse ponto vamos inicializar o contrato acionando o init, e executar as demais funções locais.

#### 4.1) Init

```bash
stellar contract invoke --id "$CONTRACT_ID" --source-account admin -- init --admin "$ADMIN_G" --xml_asset "$TOKEN_XLM_ID"
```
- Preencher o argumento `--id` com a identificação do contrato.(`$CONTRACT_ID`) do passo anterior
- Preencher o argumento `--source-account admin` para asociar a conta adm do contrato
- Preencher o argumento com o methodo do contrato que vamos acionar, nesse caso o `--init` passando `--admin "$ADMIN_G" --xml_asset "$TOKEN_XLM_ID"`. O `$ADMIN_G` possuei a chave publica da conta do admin, e passando o contrato do token que vamos utilizar `$TOKEN_XLM_ID`, atuamente o XML, mas podemos modificar para um asset proprio no futuro.

- resultado esperado:
```bash
ℹ️  Signing transaction: 0bf7352258e2eaccbbe4c8740c0aeb794f55a3d0b5e7978e845afdd3f208a3c0
📅 CAVXVNO65TXIW3V6XSXZFMISZATMKMXTB3I2X7IEKDWT4GW5NLD73B7C - Success - Event: [{"symbol":"init"},{"address":"GDATLX2VVLFQG6ZRFQ5NRMZHSYRC43YUGIKQZ7OPDB26TME3M5Y5A6VU"}] = {"address":"CDMLFMKMMD7MWZP3FKUBZPVHTUEDLSX4BYGYKH4GCESXYHS3IHQ4EIG4"}
```

#### 4.2) set_package and get_package

- O `set_package` é usado para configurar um pacote de assinatura.
- Nesse caso, estamos configurando um pacote com `id 1`, `price 100000000` (100 XLM) e `duration_secs 3600` (1 hora).

```bash
stellar contract invoke --id $CONTRACT_ID --source-account admin -- set_package --id 1 --price 100000000 --duration_secs 3600
```
- O `get_package` é usado para recuperar as informações de um pacote de assinatura.
- Nesse caso, estamos recuperando o pacote com `id 1` (configurado anteriormente).
- O resultado esperado é as informações do pacote, como `price` e `duration_secs`.
- A source account é a conta que está assinando a transação, nesse caso a conta admin. Porem pode ser utilizado outras contas para consultar os pacotes.

```bash
stellar contract invoke --id $CONTRACT_ID --source-account admin -- get_package --package_id 1
```

### 4.3) buy package (comprar pacote)

- O `buy_order` é usado para comprar um pacote de assinatura.
- Nesse caso, estamos comprando o pacote com `id 1` (configurado anteriormente) com `price 100000000` (100 XLM).

```bash
ORDER_ID=$(stellar contract invoke   --id "$CONTRACT_ID"   --source alice   -- buy_order   --owner "$ALICE_G"   --package_id 1)
echo "$ORDER_ID"
```
- Teste opcional de transferencia na rede (esse trecho simula o que estamos fazendo dentro do contrato), se não rodar esse trecho sua rede tem algum BO
```bash
stellar contract invoke --id "$TOKEN_XLM_ID" --source alice \
  -- transfer --from "$ALICE_G" --to "$ADMIN_G" --amount 10000000
```

### 4.4) build-only: gera XDR não assinado (não sai XLM)

- O `build-only` é usado para gerar o XDR (Transaction Data Representation) da transação, sem assinatura.
- Nesse caso, estamos gerando o XDR da ordem de compra com `id $ORDER_ID` (obtido anteriormente).

```bash
# gera XDR NÃO assinado (build-only)
UNSIGNED_XDR=$(
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source alice \
    --build-only \
    -- buy_order \
    --owner "$ALICE_G" \
    --package_id 1
)
```
- vc pode ver o XDR no console com `echo $UNSIGNED_XDR``

```bash
AAAAAgAAAAAUdyn5mvmHMGKDHC6ftwcEwF1sWPU6e06hV83LmISylAAAAGQAAABgAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAG8931ECaO4R/rQYUXCW72zKSoIZ59PtrdVUbImpWjAHQAAAAlidXlfb3JkZXIAAAAAAAACAAAAEgAAAAAAAAAAFHcp+Zr5hzBigxwun7cHBMBdbFj1OntOoVfNy5iEspQAAAADAAAAAQAAAAAAAAAAAAAAAA==
```

### 4.5) simulate/assinar localmente como Alice (ainda não sai XLM)

- O `simulate` é usado para simular a transação, sem assinatura.
- O `--source alice` é usado para indicar que a transação será assinada pela conta `alice`.
- Nesse caso, estamos assinando a ordem de compra com `id $ORDER_ID` (obtido anteriormente).

```bash
# simulate (prepara footprint + fees + soroban auths)
PREPARED_XDR=$(echo "$UNSIGNED_XDR" | stellar tx simulate --source-account alice)
# sign
SIGNED_XDR=$(echo "$PREPARED_XDR" | stellar tx sign --sign-with-key alice)
```

- Resultado:
```bash
ℹ️  Signing transaction: f6757e9961d72d8830f3fca8dcedd2c95daace9bc28d00adadb85c0fad38ea1f
```

### 4.6) submeter (aqui sim, se SUCESSO, o XLM sai)

- O `stellar tx send` é usado para submeter uma transação assinada para a rede.
- Nesse caso, estamos submetendo a transação assinada da ordem de compra com `id $ORDER_ID` (obtido anteriormente).

```bash
echo "$SIGNED_XDR" | stellar tx send
```
---

### 4.7) Depois da compra Admin ou user pode acionar o grant

- O `grant` é usado para conceder acesso a um usuário a um pacote de assinatura.
- Nesse caso, estamos concedendo acesso ao usuário `alice` ao pacote com `id 1` (configurado anteriormente).
- O `--caller "$ALICE_G"` é usado para indicar que o usuário `alice` está chamando a função.
- O `--owner "$ALICE_G"` é usado para indicar que o usuário `alice` é o dono do pacote.
- O `--order_id "<ORDER_ID>"` é usado para indicar que a ordem de compra com `id <ORDER_ID>` é a ordem de compra a ser usada.

```bash
stellar contract invoke --id "$CONTRACT_ID" --source alice --build-only \
  -- grant --caller "$ALICE_G" --owner "$ALICE_G" --order_id "<ORDER_ID>" \
| stellar tx simulate --source-account alice \
| stellar tx sign --sign-with-key alice \
| stellar tx send
```

## Resumão da massa

```bash
# sempre garanta a rede certa:
export STELLAR_NETWORK=local

# cria as contas
stellar keys fund alice
stellar keys fund bob

# cria o token
stellar asset create --asset-code XML --issuer admin --supply 100000000 --network local

# inicia o ledger
stellar ledger open

- Soroban (contratos): https://developers.stellar.org/docs/build/smart-contracts
- CLI (stellar-cli): https://developers.stellar.org/docs/build/smart-contracts/getting-started/cli
- Ambiente / Ledger timestamp: https://developers.stellar.org/docs/build/smart-contracts/environment#ledger
- Storage: https://developers.stellar.org/docs/build/smart-contracts/storage
- Auth: https://developers.stellar.org/docs/build/smart-contracts/auth
- Events: https://developers.stellar.org/docs/build/smart-contracts/events
- Stellar Asset Contract (SAC): https://developers.stellar.org/docs/learn/stellar-asset-contract

# build e deploy do contrato
stellar contract build --package stay-online
CONTRACT_ID=$(stellar contract deploy --wasm target/wasm32v1-none/release/stay_online.wasm --source-account admin | tail -n 1)

# inicializa o contrato
stellar contract invoke --id "$CONTRACT_ID" --source-account admin -- init --admin "$ADMIN_G" --xml_asset "$TOKEN_XLM_ID"

# configura o pacote
stellar contract invoke --id $CONTRACT_ID --source-account admin -- set_package --id 1 --price 100000000 --duration_secs 3600

# compra o pacote
ORDER_ID=$(stellar contract invoke   --id "$CONTRACT_ID"   --source alice   -- buy_order   --owner "$ALICE_G"   --package_id 1)
echo "$ORDER_ID"

# assina a ordem
UNSIGNED_XDR=$(stellar contract invoke --id "$CONTRACT_ID" \
  -- buy_order --owner "$ALICE_G" --package_id 1 --build-only)      

# preparar a ordem


# assina a ordem
SIGNED_XDR=$(echo "$UNSIGNED_XDR" | stellar tx sign --sign-with-key alice)
echo "$SIGNED_XDR"

# submete a ordem
echo "$SIGNED_XDR" | stellar tx send

# espera o ledger ser confirmado
 stellar ledger wait
```


# Rodando na testnet

## 1) set network testnet

Seta sua rede para test net

```bash
 stellar network use testnet
```

## 2) Criar os endereços que vamos utilizar

```bash
# Cria as chaves
stellar keys generate master
stellar keys generate maria

# Ver os endereços
export MASTER_ADDRESS=$(stellar keys address master)
export MARIA_ADDRESS=$(stellar keys address maria)

echo "Master: $MASTER_ADDRESS"
echo "Maria: $MARIA_ADDRESS"

```

-resultado:
```bash
Master: GB6UE3KBSV6LBP7HHSFUB2W32M6OE2EEBUUGDCLS442WKD76ED2OUIPJ
Maria: GBQXJ5OUUJO4DYJ43ZT7FFU4NM4TQGBHLEN7G553WIPQMTN4LX45ZL35
```

Consultar/add fundos da conta

```bash
# Fund das contas
curl "https://friendbot.stellar.org?addr=$MASTER_ADDRESS"
curl "https://friendbot.stellar.org?addr=$MARIA_ADDRESS"
# Verificar saldo da master via API
curl "https://horizon-testnet.stellar.org/accounts/$MASTER_ADDRESS"
# Verificar saldo da maria via API  
curl "https://horizon-testnet.stellar.org/accounts/$MARIA_ADDRESS"


# add fundos
stellar keys fund $MASTER_ADDRESS
stellar keys fund $MARIA_ADDRESS
```

## 3) Deploy contract

```bash
# Build cargo
cargo build --target wasm32v1-none --release 
#build CLI stellar
stellar contract build --package stay-online

#otimizar
stellar contract optimize --wasm target/wasm32v1-none/release/stay_online.wasm

# Deploy
export CONTRACT_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/stay_online.optimized.wasm \
  --source master \
  --network testnet)

echo "Contract deployed: $CONTRACT_ID"
```

resultado:
```bash
ℹ️  Simulating install transaction…
ℹ️  Signing transaction: 738f0b419a30c4f9a071361e318f030e7710bab344aa165504940392f8012da8
🌎 Submitting install transaction…
ℹ️  Using wasm hash 5695f536a7e5dcea41745130597bf28a93fd3585f7a0bd2f7382ef028adb630d
ℹ️  Simulating deploy transaction…
ℹ️  Transaction hash is c4bfc4b40daff2612d826ca91377c04ce0750523468290a6c789be5ff6dc5c31
🔗 https://stellar.expert/explorer/testnet/tx/c4bfc4b40daff2612d826ca91377c04ce0750523468290a6c789be5ff6dc5c31
ℹ️  Signing transaction: c4bfc4b40daff2612d826ca91377c04ce0750523468290a6c789be5ff6dc5c31
🌎 Submitting deploy transaction…
🔗 https://stellar.expert/explorer/testnet/contract/CCGDIWMT3NLGI2EZPRKWXTFWRAGN4FUX44H4FYOB2RDKJFV4M7NPCQKE
✅ Deployed!
```

Descobrir  o endereco correto do XML
```bash
export XLM_CONTRACT_ID=$(stellar contract id asset --asset native --network testnet)
echo $XLM_CONTRACT_ID

#resultado
CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC
```

## 4) Inicializar o contrato

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source master \
  --network testnet \
  -- init \
  --admin "$MASTER_ADDRESS" \
  --xml_asset "$XLM_CONTRACT_ID"

  #resultado:
  ℹ️  Signing transaction: f33e14d1a62ecc69e15bfe50b88cc34522842ce9bd57319993106951ae971fa6
  📅 CCGDIWMT3NLGI2EZPRKWXTFWRAGN4FUX44H4FYOB2RDKJFV4M7NPCQKE - Success - Event: [{"symbol":"init"},{"address":"GB6UE3KBSV6LBP7HHSFUB2W32M6OE2EEBUUGDCLS442WKD76ED2OUIPJ"}] = {"address":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"}
```

vc pode consultar o contrato no explorer

```curl
https://stellar.expert/explorer/testnet/contract/CCGDIWMT3NLGI2EZPRKWXTFWRAGN4FUX44H4FYOB2RDKJFV4M7NPCQKE
```

## 5) Criar/consultar pacotes

### Criar Pacotes
```bash
# Package 1: mais barato, menos tempo
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source master \
  --network testnet \
  -- set_package \
  --id 1 \
  --price 500000 \
  --duration_secs 1800

  #resultado ex:
  ℹ️  Signing transaction: 430d5fdeb1f68e24a9d8597acbabbe9a85b886b3dcfcb6d0eead7206004c1879
  📅 CCGDIWMT3NLGI2EZPRKWXTFWRAGN4FUX44H4FYOB2RDKJFV4M7NPCQKE - Success - Event: [{"symbol":"pkg_set"},{"u32":1}] = {"vec":[{"i128":"500000"},{"u32":1800}]}
````
```bash
# Package 2: mais caro, mais tempo  
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source master \
  --network testnet \
  -- set_package \
  --id 2 \
  --price 2000000 \
  --duration_secs 7200
```

### Consultar pacote

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source master \
  --network testnet \
  -- get_package \
  --package_id 1

  #resultado:
  ℹ️  Simulation identified as read-only. Send by rerunning with `--send=yes`.
   {"duration_secs":1800,"price":"500000"}
```

## 6) Comprar pacote

### Vamos consultar o saldo da Maria

```bash
stellar contract invoke \
  --id "$XLM_CONTRACT_ID" \
  --source maria \
  --network testnet \
  -- balance \
  --id "$MARIA_ADDRESS"
  ```

#### Vamos consutlar o pacote existe

```bash
stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source maria \
  --network testnet \
  -- get_package \
  --package_id 1
```

### Comprar pacote

```bash
ORDER_ID=$(stellar contract invoke \
  --id "$CONTRACT_ID" \
  --source maria \
  --network testnet \
  -- buy_order \
  --owner "$MARIA_ADDRESS" \
  --package_id 1)

echo "Order ID: $ORDER_ID"

```
resultado:
```bash
ℹ️  Signing transaction: e3fe402260a5d7b1fbea74ee62b910abb59fbf9344f592aa0295b9303cd713fc
📅 CBRJH5RWZ4JQ5DDYOQV7CUUBPRQIVAJNJ5PQ7STGOAVQDC447C3PAA5X - Success - Event: [{"symbol":"dbg"},{"symbol":"start"}] = "void"
📅 CBRJH5RWZ4JQ5DDYOQV7CUUBPRQIVAJNJ5PQ7STGOAVQDC447C3PAA5X - Success - Event: [{"symbol":"dbg"},{"symbol":"before_transfer"}] = "void"
📅 CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC - Success - Event: [{"symbol":"transfer"},{"address":"GBQXJ5OUUJO4DYJ43ZT7FFU4NM4TQGBHLEN7G553WIPQMTN4LX45ZL35"},{"address":"GB6UE3KBSV6LBP7HHSFUB2W32M6OE2EEBUUGDCLS442WKD76ED2OUIPJ"},{"string":"native"}] = {"i128":"500000"}
📅 CBRJH5RWZ4JQ5DDYOQV7CUUBPRQIVAJNJ5PQ7STGOAVQDC447C3PAA5X - Success - Event: [{"symbol":"dbg"},{"symbol":"after_transfer"}] = "void"
📅 CBRJH5RWZ4JQ5DDYOQV7CUUBPRQIVAJNJ5PQ7STGOAVQDC447C3PAA5X - Success - Event: [{"symbol":"purchase"},{"symbol":"created"}] = {"vec":[{"address":"GBQXJ5OUUJO4DYJ43ZT7FFU4NM4TQGBHLEN7G553WIPQMTN4LX45ZL35"},{"u32":1},{"u128":"1"},{"i128":"500000"}]}
📅 CBRJH5RWZ4JQ5DDYOQV7CUUBPRQIVAJNJ5PQ7STGOAVQDC447C3PAA5X - Success - Event: [{"symbol":"dbg"},{"symbol":"done"}] = "void"
Order ID: "1"
```


Consultar transaction
```bash
#consulta transação
curl "https://horizon-testnet.stellar.org/transactions/fad340b5a62535a394d7ff4fbb4b42f0aa7ef8d1730ea2c1b4735e5b60ecb5ca/effects"
```


## 7) Criar assinatura em etapas, compra, depois assinatura do XDR

### Gerar XDR
Porque usar esse fluxo?
Estamos simulando um ambiente real, onde o cliente entra pelo FRONT, aciona a API de compra via back end, que vai montar o XDR com a simulação de compra para o cliente. Exemplo abaixo:

```bash
# 
echo "=== BACKEND: Preparando transação ==="
UNSIGNED_XDR=$(stellar contract invoke \
  --id $CONTRACT_ID \
  --source-account maria \
  --network testnet \
  --build-only \
  -- buy_order \
  --owner $MARIA_ADDRESS \
  --package_id 1)

```

resultado:
```bash
echo "XDR não assinado: $UNSIGNED_XDR"
=== BACKEND: Preparando transação ===
XDR não assinado: AAAAAgAAAABhdPXUol3B4TzeZ/KWnGs5OBgnWRvzd7uyHwZNvF353AAAAGQABYIBAAAABwAAAAAAAAAAAAAAAQAAAAAAAAAYAAAAAAAAAAFik/Y2zxMOjHh0K/FSgXxgioEtT18PymZwKwGLnPi28AAAAAlidXlfb3JkZXIAAAAAAAACAAAAEgAAAAAAAAAAYXT11KJdweE83mfylpxrOTgYJ1kb83e7sh8GTbxd+dwAAAADAAAAAQAAAAAAAAAAAAAAAA==
```

### Simular Wallat(assinar XDR)

Nessa etapa, o cliente vai usar a wallet para assinar o XDR, que foi gerado no back end.
```bash

echo "=== FRONTEND: Simulando (footprint + fees) ==="
PREPARED_XDR=$(echo "$UNSIGNED_XDR" | stellar tx simulate --source-account maria)

echo "=== WALLET: Assinando ==="
SIGNED_XDR=$(echo "$PREPARED_XDR" | stellar tx sign --sign-with-key maria)

echo "=== BLOCKCHAIN: Submetendo ==="
echo "$SIGNED_XDR" | stellar tx send
```
