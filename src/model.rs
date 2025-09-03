use soroban_sdk::{contracttype, contracterror, Address};
// -------------------------------------------------------------
// MODELO DE DADOS
// -------------------------------------------------------------

/// Um "pacote" de internet: preço (em XLM) e duração (em segundos).
#[derive(Clone)]
#[contracttype]
pub struct Package {
    pub price: i128,     // preço em unidades do token "XML"
    pub duration_secs: u32,  // duração total concedida ao comprar este pacote
}

/// Estado de sessão com "saldo de segundos" e marcador de início:
/// - Quando started_at == 0 -> pausado (saldo congelado)
/// - Quando started_at  > 0 -> consumindo desde 'started_at'
#[derive(Clone)]
#[contracttype]
pub struct Session {
    pub remaining_secs: u64, // saldo de segundos "congeláveis"
    pub started_at: u64,     // unix ts (ledger). 0 = pausado
}

/// Estrutura compatível com o modelo "expira em" caso você queira expor
/// um "expires_at virtual" quando a sessão está ativa.
#[derive(Clone)]
#[contracttype]
pub struct Access {
    pub owner: Address,
    pub expires_at: u64, // se pausado, 0; se ativo, started_at + remaining_secs
}

/// Chaves de armazenamento:
/// - Instance storage: Admin/Token/Package -> configuração global do contrato
/// - Persistent storage: Session(owner)     -> estado por usuário (vida longa)
#[contracttype]
pub enum DataKey {
    Admin,               // Address do administrador do catálogo
    Token,               // Address do contrato do token (SAC) do "XLM"
    Package(u32),        // id -> Package
    Session(Address),    // owner -> Session
}

// -------------------------------------------------------------
// ERROS
// -------------------------------------------------------------
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    PackageNotFound = 4,
    InsufficientBalance = 5,
}