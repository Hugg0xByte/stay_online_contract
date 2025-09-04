use crate::model::{Access, DataKey, Error, OrderRec, Package, Session};

use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token::Client as TokenClient, Address,
    Env, Symbol,
};

// -------------------------------------------------------------
// TIPOS AUXILIARES (somente neste arquivo)
// -------------------------------------------------------------

// storage helpers p/ OrderRec (persistent)
fn load_order(env: &Env, owner: &Address, order_id: u128) -> Option<OrderRec> {
    env.storage()
        .persistent()
        .get::<_, OrderRec>(&DataKey::Order(owner.clone(), order_id))
}
fn save_order(env: &Env, owner: &Address, order_id: u128, rec: &OrderRec) {
    env.storage()
        .persistent()
        .set(&DataKey::Order(owner.clone(), order_id), rec);
}

// helper p/ contador determinístico (instance): NextOrder(owner) -> u128
fn next_order_id(env: &Env, owner: &Address) -> u128 {
    let key = DataKey::NextOrder(owner.clone());
    let current: u128 = env.storage().instance().get(&key).unwrap_or(0);
    let next = current + 1;
    env.storage().instance().set(&key, &next);
    next
}

// -------------------------------------------------------------
// HELPERS (tempo)
// -------------------------------------------------------------
fn remaining_at(_env: &Env, s: &Session, now: u64) -> u64 {
    if s.started_at == 0 {
        s.remaining_secs
    } else {
        s.remaining_secs
            .saturating_sub(now.saturating_sub(s.started_at))
    }
}
fn load_session(env: &Env, owner: &Address) -> Session {
    env.storage()
        .persistent()
        .get::<_, Session>(&DataKey::Session(owner.clone()))
        .unwrap_or(Session {
            remaining_secs: 0,
            started_at: 0,
        })
}

fn save_session(env: &Env, owner: &Address, s: &Session) {
    env.storage()
        .persistent()
        .set(&DataKey::Session(owner.clone()), s);
}

// -------------------------------------------------------------
// EVENTOS
// -------------------------------------------------------------
// fn emit_purchase_created(env: &Env, owner: &Address, package_id: u32, order_id: u128, price: i128) {
//     env.events().publish(
//         (symbol_short!("purchase"), symbol_short!("created")),
//         (owner, package_id, order_id, price),
//     );
// }
fn emit_grant(env: &Env, owner: &Address, order_id: u128, new_remaining: u64) {
    env.events()
        .publish((symbol_short!("grant"), owner, order_id), new_remaining);
}

// -------------------------------------------------------------
// CONTRATO
// -------------------------------------------------------------
#[contract]
pub struct AccessTime;

#[contractimpl]
impl AccessTime {
    // -------------------- init / set_package (inalterados) --------------------
    pub fn init(env: Env, admin: Address, xml_asset: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Token, &xml_asset);
        env.events()
            .publish((symbol_short!("init"), admin.clone()), xml_asset);
    }

    pub fn set_package(env: Env, id: u32, price: i128, duration_secs: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
            .unwrap();
        admin.require_auth();
        let pkg = Package {
            price,
            duration_secs,
        };
        env.storage().instance().set(&DataKey::Package(id), &pkg);
        env.events()
            .publish((symbol_short!("pkg_set"), id), (price, duration_secs));
    }

    // Add this function to the contract implementation
    pub fn get_package(env: Env, package_id: u32) -> Package {
        env.storage()
            .instance()
            .get(&DataKey::Package(package_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::PackageNotFound))
    }
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized))
    }

    pub fn get_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized))
    }

    // -------------------- NOVO: buy_order (compra sem crédito) ----------------

    fn dbg(env: &Env, step: &str) {
        env.events()
            .publish((Symbol::new(env, "dbg"), Symbol::new(env, step)), ());
    }

    pub fn buy_order(env: Env, owner: Address, package_id: u32) -> u128 {
        owner.require_auth();
        Self::dbg(&env, "start");

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| {
                Self::dbg(&env, "err_no_admin");
                panic_with_error!(&env, Error::NotInitialized)
            });

        let token_id: Address = env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .unwrap_or_else(|| {
                Self::dbg(&env, "err_no_token");
                panic_with_error!(&env, Error::NotInitialized)
            });

        let pkg: Package = env
            .storage()
            .instance()
            .get(&DataKey::Package(package_id))
            .unwrap_or_else(|| {
                Self::dbg(&env, "err_pkg_nf");
                panic_with_error!(&env, Error::PackageNotFound)
            });

        Self::dbg(&env, "before_transfer");
        let token = TokenClient::new(&env, &token_id);
        token.transfer(&owner, &admin, &pkg.price); // se der erro do SAC, diagnostics mostram

        Self::dbg(&env, "after_transfer");

        // >>>>> ALTERAÇÃO: gerar order_id determinístico pelo contador <<<<<
        let order_id: u128 = next_order_id(&env, &owner);

        save_order(
            &env,
            &owner,
            order_id,
            &OrderRec {
                package_id,
                credited: false,
            },
        );

        env.events().publish(
            (Symbol::new(&env, "purchase"), Symbol::new(&env, "created")),
            (owner, package_id, order_id, pkg.price),
        );
        Self::dbg(&env, "done");
        order_id
    }

    // -------------------- NOVO: grant (owner OU admin) ------------------------
    /// Credita os segundos do pacote na sessão do `owner` referentes à `order_id`.
    /// Pode ser chamado pelo **owner** (self-serve) OU pelo **admin**.
    /// - Idempotente: se já creditado, retorna erro `AlreadyGranted`.
    pub fn grant(env: Env, caller: Address, owner: Address, order_id: u128) {
        // autoriza: caller deve ser admin OU o próprio owner
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
            .unwrap();
        if caller != admin && caller != owner {
            panic_with_error!(&env, Error::Unauthorized);
        }
        caller.require_auth();

        // busca ordem; precisa existir e não ter sido creditada
        let mut ord = load_order(&env, &owner, order_id)
            .ok_or(Error::OrderNotFound)
            .unwrap();
        if ord.credited {
            panic_with_error!(&env, Error::AlreadyGranted);
        }

        // pega pacote vinculado à ordem
        let pkg: Package = env
            .storage()
            .instance()
            .get(&DataKey::Package(ord.package_id))
            .ok_or(Error::PackageNotFound)
            .unwrap();

        // credita tempo (não inicia)
        let mut s = load_session(&env, &owner);
        s.remaining_secs = s.remaining_secs.saturating_add(pkg.duration_secs as u64);
        save_session(&env, &owner, &s);

        // marca como creditado e emite evento
        ord.credited = true;
        save_order(&env, &owner, order_id, &ord);
        emit_grant(&env, &owner, order_id, s.remaining_secs);
    }

    // -------------------- start / pause / getters (inalterados) ---------------
    pub fn start(env: Env, owner: Address) {
        owner.require_auth();
        let now = env.ledger().timestamp();
        let mut s = load_session(&env, &owner);
        if remaining_at(&env, &s, now) == 0 {
            return;
        }
        if s.started_at == 0 {
            s.started_at = now;
            save_session(&env, &owner, &s);
            env.events().publish((symbol_short!("start"), owner), now);
        }
    }

    pub fn pause(env: Env, owner: Address) {
        owner.require_auth();
        let now = env.ledger().timestamp();
        let mut s = load_session(&env, &owner);
        if s.started_at > 0 {
            let gasto = now.saturating_sub(s.started_at);
            s.remaining_secs = s.remaining_secs.saturating_sub(gasto);
            s.started_at = 0;
            save_session(&env, &owner, &s);
            env.events()
                .publish((symbol_short!("pause"), owner), s.remaining_secs);
        }
    }

    pub fn get_session(env: Env, owner: Address) -> Session {
        load_session(&env, &owner)
    }

    pub fn get_access(env: Env, owner: Address) -> Access {
        let s = load_session(&env, &owner);
        let ea = if s.started_at > 0 {
            s.started_at.saturating_add(s.remaining_secs)
        } else {
            0
        };
        Access {
            owner,
            expires_at: ea,
        }
    }

    pub fn is_active(env: Env, owner: Address, now: u64) -> bool {
        let s = load_session(&env, &owner);
        s.started_at > 0 && remaining_at(&env, &s, now) > 0
    }

    pub fn remaining(env: Env, owner: Address, now: u64) -> u64 {
        let s = load_session(&env, &owner);
        remaining_at(&env, &s, now)
    }
}
