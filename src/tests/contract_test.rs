#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use crate::contract::{AccessTime, AccessTimeClient};
use crate::model::Session;

#[test]
fn test_buy_order_and_grant_flow() {
    let env = Env::default();
    env.mock_all_auths();

    // Set initial timestamp
    env.ledger().with_mut(|li| li.timestamp = 1000000);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup SAC and contract
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    // Initialize contract and create package
    client.init(&admin, &token_id);
    client.set_package(&2u32, &50_i128, &7200u32); // package_id 2, price 50, 2 hours

    // Mint tokens to user
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    // Test buy_order
    let order_id = client.buy_order(&user, &2u32);
    assert!(order_id > 0);

    // Session should still be empty (not credited yet)
    let session = client.get_session(&user);
    assert_eq!(session.remaining_secs, 0);
    assert_eq!(session.started_at, 0);

    // Grant the order (can be called by user or admin)
    client.grant(&user, &user, &order_id); // user grants to themselves

    // Now session should have the time credited
    let session_after_grant = client.get_session(&user);
    assert_eq!(session_after_grant.remaining_secs, 7200);
    assert_eq!(session_after_grant.started_at, 0); // still paused

    // Verify user's token balance decreased
    let token_client = TokenClient::new(&env, &token_id);
    let user_balance = token_client.balance(&user);
    assert_eq!(user_balance, 950_i128); // 1000 - 50
}

#[test]
#[should_panic]
fn test_buy_order_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);
    client.set_package(&1u32, &100_i128, &3600u32); // expensive package

    // Mint insufficient tokens
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &50_i128); // only 50, but package costs 100

    // This should panic with insufficient balance
    client.buy_order(&user, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_buy_order_package_not_found() {
    let env = Env::default();
    env.mock_all_auths();

    // Set initial timestamp
    env.ledger().with_mut(|li| li.timestamp = 1000000);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);
    // Don't create package_id 99

    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    // This should panic with package not found
    client.buy_order(&user, &99u32); // non-existent package
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_grant_twice_should_fail() {
    let env = Env::default();
    env.mock_all_auths();

    // Set initial timestamp
    env.ledger().with_mut(|li| li.timestamp = 1000000);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);
    client.set_package(&1u32, &25_i128, &1800u32);

    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    // Buy and grant
    let order_id = client.buy_order(&user, &1u32);
    client.grant(&user, &user, &order_id);

    // Try to grant again - should panic
    client.grant(&user, &user, &order_id);
}

#[test]
fn test_admin_can_grant_user_order() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);
    client.set_package(&1u32, &30_i128, &2400u32);

    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    // User buys order
    let order_id = client.buy_order(&user, &1u32);

    // Admin grants the order (instead of user)
    client.grant(&admin, &user, &order_id);

    // Verify time was credited
    let session = client.get_session(&user);
    assert_eq!(session.remaining_secs, 2400);
}

#[test]
fn test_debug_buy_order_package_2() {
    let env = Env::default();
    env.mock_all_auths();

    // Set initial timestamp
    env.ledger().with_mut(|li| li.timestamp = 1000000);

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);

    // Setup exactly like your deployment
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);

    // Create package_id 2 with some price
    client.set_package(&2u32, &100_i128, &3600u32);

    // Mint tokens to alice
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&alice, &1000_i128);

    // Check alice's balance before
    let token_client = TokenClient::new(&env, &token_id);
    let balance_before = token_client.balance(&alice);
    assert!(balance_before > 0);

    // Try buy_order
    let order_id = client.buy_order(&alice, &2u32);
    assert!(order_id > 0);

    // Check balance after
    let balance_after = token_client.balance(&alice);
    assert!(balance_after < balance_before);
}

// Keep the original test too
#[test]
fn buy_start_pause_flow() {
    let env = Env::default();
    env.mock_all_auths(); // autorizações "ok" no ambiente de teste

    // Set initial timestamp
    env.ledger().with_mut(|li| li.timestamp = 1000000);

    // Endereços de teste
    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // 1) Registra um SAC de teste e obtém o Address do token
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = sac.address();

    // 2) Registra nosso contrato e cria o client auto-gerado
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    // 3) init + pacote "10 XML = 1h"
    client.init(&admin, &token_id);
    client.set_package(&1u32, &10_i128, &3600u32);

    // 4) mint de saldo para o usuário no SAC (admin é emissor nos testes)
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &100_i128);

    // 5) compra - NOTE: This should be buy_order if you want to test the new flow
    // client.buy(&user, &1u32); // This function doesn't exist in your contract
    let order_id = client.buy_order(&user, &1u32);
    client.grant(&user, &user, &order_id);

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
    env.ledger().with_mut(|li| {
        li.timestamp = t0 + 120;
    }); // move tempo no teste

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
    env.ledger().with_mut(|li| {
        li.timestamp = now + 600;
    });
    let still = client.remaining(&user, &(now + 600));
    assert_eq!(still, frozen);

    // 9) buy enquanto "pausado" -> só soma
    let order_id2 = client.buy_order(&user, &1u32);
    client.grant(&user, &user, &order_id2);
    s = client.get_session(&user);
    assert_eq!(s.started_at, 0);
    assert!(s.remaining_secs >= frozen + 3600);

    // 10) start novamente e checa is_active
    client.start(&user);
    let now2 = env.ledger().timestamp();
    assert!(client.is_active(&user, &now2));
}

#[test]
fn test_debug_buy_order_step_by_step() {
    let env = Env::default();
    env.mock_all_auths();

    // Set initial timestamp
    env.ledger().with_mut(|li| {
        li.timestamp = 1000000;
    });

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Setup SAC and contract
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    // Debug: Check if init works
    client.init(&admin, &token_id);

    // Debug: Check if package creation works
    client.set_package(&2u32, &50_i128, &7200u32);

    // Debug: Check token minting
    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    let token_client = TokenClient::new(&env, &token_id);
    let balance = token_client.balance(&user);
    assert_eq!(balance, 1000_i128);

    // Debug: Check timestamp
    let timestamp = env.ledger().timestamp();
    assert!(timestamp > 0);

    // Now try buy_order and see what happens
    let order_id = client.buy_order(&user, &2u32);

    // Debug: print the actual value in the assertion message
    assert!(order_id > 0, "order_id was {}, expected > 0", order_id);
}

#[test]
fn test_minimal_buy_order() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().with_mut(|li| li.timestamp = 1000000);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_id = env.register(AccessTime, ());
    let client = AccessTimeClient::new(&env, &contract_id);

    client.init(&admin, &token_id);
    client.set_package(&2u32, &50_i128, &7200u32);

    let token_admin = StellarAssetClient::new(&env, &token_id);
    token_admin.mint(&user, &1000_i128);

    // Just call it and see if it works
    let order_id = client.buy_order(&user, &2u32);
    assert!(order_id > 0, "order_id was {}", order_id);
}
