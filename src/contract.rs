use crate::model::{Access,Package,Error,Session, DataKey};

// Soroban SDK: macros do contrato, tipos Address/Env/Storage, eventos e cliente de token (SAC).
use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Env,
    panic_with_error,
    token::Client as TokenClient,
};


// -------------------------------------------------------------
// HELPERS (cálculo de tempo e acesso ao storage)
// -------------------------------------------------------------

/// Calcula o saldo "em tempo real":
/// - se pausado: remaining_secs
/// - se ativo:   remaining_secs - (now - started_at)  (com saturação)
fn remaining_at(_env: &Env, s: &Session, now: u64) -> u64 {
    if s.started_at == 0 {
        s.remaining_secs
    } else {
        s.remaining_secs.saturating_sub(now.saturating_sub(s.started_at))
    }
}

fn load_session(env: &Env, owner: &Address) -> Session {
    env.storage()
        .persistent()
        .get::<_, Session>(&DataKey::Session(owner.clone()))
        .unwrap_or(Session { remaining_secs: 0, started_at: 0 })
}

fn save_session(env: &Env, owner: &Address, s: &Session) {
    env.storage().persistent().set(&DataKey::Session(owner.clone()), s);
}

// -------------------------------------------------------------
// CONTRATO
// -------------------------------------------------------------
#[contract]
pub struct AccessTime;

#[contractimpl]
impl AccessTime {
    // ---------------------------------------------------------
    // init(admin, xml_asset)
    //
    // Liga o contrato ao "admin" (quem gerencia pacotes) e ao
    // "xml_asset" (endereço do SAC do token a ser cobrado).
    // Executa uma única vez.
    // ---------------------------------------------------------
    pub fn init(env: Env, admin: Address, xml_asset: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &xml_asset);

        // Evento para auditoria e ingestão off-chain
        env.events().publish((symbol_short!("init"), admin.clone()), xml_asset);
    }

    // ---------------------------------------------------------
    // set_package(id, price_xlm, duration_secs)
    //
    // Cria/atualiza um pacote. Somente o admin pode chamar.
    // Armazena em instance storage por ser configuração global.
    // ---------------------------------------------------------
    pub fn set_package(env: Env, id: u32, price: i128, duration_secs: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
            .unwrap();

        // Garante que a transação foi assinada pelo admin
        admin.require_auth();

        let pkg = Package { price, duration_secs };
        env.storage().instance().set(&DataKey::Package(id), &pkg);

        // Evento: catálogo atualizado
        env.events().publish((symbol_short!("pkg_set"), id), (price, duration_secs));
    }

    // ---------------------------------------------------------
    // buy(owner, package_id)
    //
    // Cobra via Token Interface (SAC), converte o valor em "segundos"
    // e credita no saldo do usuário. Se estiver “ativo”, desconta o
    // uso até "agora" e mantém ativo.
    // ---------------------------------------------------------
    pub fn buy(env: Env, owner: Address, package_id: u32) {
        // O dono deve assinar a compra (evita comprar em nome de terceiros)
        owner.require_auth();

        let admin: Address = env.storage().instance().get(&DataKey::Admin).ok_or(Error::NotInitialized).unwrap();
        let token_id: Address = env.storage().instance().get(&DataKey::Token).ok_or(Error::NotInitialized).unwrap();
        let pkg: Package = env.storage().instance().get(&DataKey::Package(package_id)).ok_or(Error::PackageNotFound).unwrap();

        // Cliente do token (SAC). A transferência respeita trustline/saldo e exige auth do owner.
        let token = TokenClient::new(&env, &token_id);

        // (Opcional) erro amigável se não houver saldo suficiente
        if token.balance(&owner) < pkg.price {
            panic_with_error!(&env, Error::InsufficientBalance);
        }

        // Transfere owner -> admin (cobrança)
        token.transfer(&owner, &admin, &pkg.price);

        // Tempo confiável: timestamp do ledger
        let now = env.ledger().timestamp();

        // Atualiza sessão: se estava ativo, desconta o gasto até agora e mantém ativo
        let mut s = load_session(&env, &owner);
        if s.started_at > 0 {
            let gasto = now.saturating_sub(s.started_at);
            s.remaining_secs = s.remaining_secs.saturating_sub(gasto);
            s.started_at = now; // continua ligado a partir de "agora"
        }
        // Soma os segundos do pacote
        s.remaining_secs = s.remaining_secs.saturating_add(pkg.duration_secs as u64);

        save_session(&env, &owner, &s);

        // Evento de compra (útil para liberar acesso no backend)
        env.events().publish(
            (symbol_short!("purchase"), owner, package_id),
            (pkg.price, s.remaining_secs),
        );
    }

    // ---------------------------------------------------------
    // start(owner)
    //
    // Inicia/retoma consumo do saldo (se tiver saldo > 0).
    // ---------------------------------------------------------
    pub fn start(env: Env, owner: Address) {
        owner.require_auth();
        let now = env.ledger().timestamp();
        let mut s = load_session(&env, &owner);

        if remaining_at(&env, &s, now) == 0 {
            return; // nada a consumir
        }
        if s.started_at == 0 {
            s.started_at = now;
            save_session(&env, &owner, &s);
            env.events().publish((symbol_short!("start"), owner), now);
        }
    }

    // ---------------------------------------------------------
    // pause(owner)
    //
    // Pausa o consumo (congela saldo): desconta o uso até "agora"
    // e zera 'started_at'.
    // ---------------------------------------------------------
    pub fn pause(env: Env, owner: Address) {
        owner.require_auth();
        let now = env.ledger().timestamp();
        let mut s = load_session(&env, &owner);

        if s.started_at > 0 {
            let gasto = now.saturating_sub(s.started_at);
            s.remaining_secs = s.remaining_secs.saturating_sub(gasto);
            s.started_at = 0;
            save_session(&env, &owner, &s);
            env.events().publish((symbol_short!("pause"), owner), s.remaining_secs);
        }
    }

    // ---------------------------------------------------------
    // get_session(owner) -> Session
    //
    // Retorna o estado bruto (saldo + started_at) do usuário.
    // ---------------------------------------------------------
    pub fn get_session(env: Env, owner: Address) -> Session {
        load_session(&env, &owner)
    }

    // ---------------------------------------------------------
    // get_access(owner) -> Access
    //
    // Retorna um "expires_at virtual": se estiver ativo, é
    // started_at + remaining_secs; se pausado, 0.
    // Útil para manter compatibilidade com UIs que esperam "expira em".
    // ---------------------------------------------------------
    pub fn get_access(env: Env, owner: Address) -> Access {
        let s = load_session(&env, &owner);
        let ea = if s.started_at > 0 { s.started_at.saturating_add(s.remaining_secs) } else { 0 };
        Access { owner, expires_at: ea }
    }

    // ---------------------------------------------------------
    // is_active(owner, now) -> bool
    //
    // Ativo = está em sessão (started_at > 0) e ainda tem saldo > 0.
    // ---------------------------------------------------------
    pub fn is_active(env: Env, owner: Address, now: u64) -> bool {
        let s = load_session(&env, &owner);
        s.started_at > 0 && remaining_at(&env, &s, now) > 0
    }

    // ---------------------------------------------------------
    // remaining(owner, now) -> u64
    //
    // Segundos restantes (considerando se está pausado/ativo).
    // ---------------------------------------------------------
    pub fn remaining(env: Env, owner: Address, now: u64) -> u64 {
        let s = load_session(&env, &owner);
        remaining_at(&env, &s, now)
    }
}
