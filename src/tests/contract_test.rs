#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::{AccessTime, AccessTimeClient, Package, Session};

#[test]
fn buy_start_pause_flow() {
    let env = Env::default();
    env.mock_all_auths(); // autorizações "ok" no ambiente de teste

    // Endereços de teste
    let admin = Address::generate(&env);
    let user  = Address::generate(&env);

    // 1) Registra um SAC de teste e obtém o Address do token
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = sac.contract_id();

    // 2) Registra nosso contrato e cria o client auto-gerado
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    // 3) init + pacote "10 XML = 1h"
    client.init(&admin, &token_id);
    client.set_package(&1u32, &10_i128, &3600u32);

    // 4) mint de saldo para o usuário no SAC (admin é emissor nos testes)
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &100_i128);

    // 5) compra
    client.buy(&user, &1u32);

    // saldo de segundos: 3600 (e ainda pausado)
    let mut s: Session = client.get_session(&user);
    assert_eq!(s.started_at, 0);
    assert_eq!(s.remaining_secs, 3600);

    // 6) start -> começa a correr
    client.start(&user);
    s = client.get_session(&user);
    assert!(s.started_at > 0);

    // Avança o relógio do ledger em 120s
    let t0 = env.ledger().timestamp();
    env.ledger().with_mut(|li| { li.timestamp = t0 + 120; }); // move tempo no teste

    // 7) remaining deve ter reduzido ~120s
    let now = env.ledger().timestamp();
    let remaining = client.remaining(&user, &now);
    assert!(remaining <= 3600 - 120);

    // 8) pause -> congela o saldo
    client.pause(&user);
    s = client.get_session(&user);
    assert_eq!(s.started_at, 0);
    let frozen = s.remaining_secs;

    // Avança mais 600s e confirma que continua igual porque está pausado
    env.ledger().with_mut(|li| { li.timestamp = now + 600; });
    let still = client.remaining(&user, &(now + 600));
    assert_eq!(still, frozen);

    // 9) buy enquanto "pausado" -> só soma
    client.buy(&user, &1u32);
    s = client.get_session(&user);
    assert_eq!(s.started_at, 0);
    assert!(s.remaining_secs >= frozen + 3600);

    // 10) start novamente e checa is_active
    client.start(&user);
    let now2 = env.ledger().timestamp();
    assert!(client.is_active(&user, &now2));
}
