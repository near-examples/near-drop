use anyhow::Context;
use near_api::types::SecretKey;
use near_api::{AccountId, Contract, NetworkConfig, Signer};
use near_contract_standards::non_fungible_token::TokenId;
use near_sandbox::Sandbox;
use near_sdk::{serde_json::json, NearToken};
use std::sync::{Arc, LazyLock};

static WASMS: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    [
        ".",
        "tests/contracts/ft",
        "tests/contracts/failing_ft",
        "tests/contracts/nft",
    ]
    .into_iter()
    .map(|path| {
        let artifact = cargo_near_build::build_with_cli(cargo_near_build::BuildOpts {
            manifest_path: Some(format!("{path}/Cargo.toml").into()),
            no_abi: true,
            no_embed_abi: true,
            ..Default::default()
        })
        .expect("build source contract");
        std::fs::read(artifact).expect("read built contract")
    })
    .collect()
});

#[derive(Clone)]
pub struct TestAccount {
    id: AccountId,
    pub network: NetworkConfig,
    pub signer: Arc<Signer>,
    sandbox: Arc<Sandbox>,
}
pub struct TestSandbox {
    sandbox: Arc<Sandbox>,
    root: TestAccount,
    pub network: NetworkConfig,
}
pub async fn sandbox() -> anyhow::Result<TestSandbox> {
    LazyLock::force(&WASMS);
    let sandbox = Arc::new(Sandbox::start_sandbox().await?);
    let network = NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);
    let signer = Signer::from_secret_key(
        near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY.parse()?,
    )?;
    let root = TestAccount {
        id: near_sandbox::config::DEFAULT_GENESIS_ACCOUNT.to_owned(),
        network,
        signer,
        sandbox: sandbox.clone(),
    };
    Ok(TestSandbox {
        network: root.network.clone(),
        sandbox,
        root,
    })
}
impl TestSandbox {
    pub fn root_account(&self) -> anyhow::Result<TestAccount> {
        Ok(self.root.clone())
    }
    pub async fn fast_forward(&self, blocks: u64) -> anyhow::Result<()> {
        self.sandbox.fast_forward(blocks).await?;
        Ok(())
    }
    async fn deploy_fixture(&self, name: &str, wasm: &[u8]) -> anyhow::Result<TestAccount> {
        let account = self
            .root
            .create_subaccount(name, NearToken::from_near(10))
            .await?;
        deploy(&account, wasm).await?;
        Ok(account)
    }
}
impl TestAccount {
    pub fn id(&self) -> &AccountId {
        &self.id
    }
    pub fn from_secret_key(id: AccountId, key: SecretKey, worker: &TestSandbox) -> Self {
        Self {
            id,
            network: worker.root.network.clone(),
            signer: Signer::from_secret_key(key).expect("valid claim key"),
            sandbox: worker.sandbox.clone(),
        }
    }
    pub async fn create_subaccount(&self, name: &str, balance: NearToken) -> anyhow::Result<Self> {
        let id: AccountId = format!("{name}.{}", self.id).parse()?;
        self.sandbox
            .create_account(id.clone())
            .initial_balance(balance)
            .send()
            .await
            .context("create test account")?;
        // Sandbox patches state before the account is visible at every RPC reference.
        let public_key = near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY
            .parse::<SecretKey>()?
            .public_key();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if near_api::Account(id.clone())
                    .access_key(public_key.clone())
                    .fetch_from(&self.network)
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .context("new account key did not become visible")?;
        Ok(Self {
            id,
            network: self.network.clone(),
            signer: self.signer.clone(),
            sandbox: self.sandbox.clone(),
        })
    }
}
async fn deploy(account: &TestAccount, wasm: &[u8]) -> anyhow::Result<()> {
    Contract::deploy(account.id.clone())
        .use_code(wasm.to_vec())
        .without_init_call()
        .with_signer(account.signer.clone())
        .send_to(&account.network)
        .await
        .context("deploy source contract")?
        .assert_success();
    Ok(())
}
async fn initialize(
    account: &TestAccount,
    args: serde_json::Value,
    method: &str,
) -> anyhow::Result<()> {
    Contract(account.id.clone())
        .call_function(method, args)
        .transaction()
        // Keep initialization affordable for the deliberately underfunded reserve test.
        .gas(near_sdk::Gas::from_tgas(if method == "new" {
            10
        } else {
            100
        }))
        .with_signer(account.id.clone(), account.signer.clone())
        .send_to(&account.network)
        .await
        .context("initialize source contract")?
        .assert_success();
    Ok(())
}
pub async fn init(
    root: &TestAccount,
    initial_contract_balance: NearToken,
) -> anyhow::Result<(TestAccount, TestAccount, TestAccount)> {
    let contract = root
        .create_subaccount("contract", initial_contract_balance)
        .await?;
    deploy(&contract, &WASMS[0]).await?;
    let creator = root
        .create_subaccount("creator", NearToken::from_near(5))
        .await?;
    let alice = root
        .create_subaccount("alice", NearToken::from_millinear(100))
        .await?;
    initialize(&contract, json!({}), "new").await?;
    Ok((contract, creator, alice))
}
pub async fn init_ft_contract(
    worker: &TestSandbox,
    creator: &TestAccount,
) -> anyhow::Result<TestAccount> {
    init_ft(worker, creator, "ft", 1).await
}
pub async fn init_failing_ft_contract(
    worker: &TestSandbox,
    creator: &TestAccount,
) -> anyhow::Result<TestAccount> {
    init_ft(worker, creator, "failing-ft", 2).await
}
async fn init_ft(
    worker: &TestSandbox,
    creator: &TestAccount,
    name: &str,
    index: usize,
) -> anyhow::Result<TestAccount> {
    let ft = worker.deploy_fixture(name, &WASMS[index]).await?;
    initialize(&ft, json!({"owner_id": creator.id(), "name":"token", "symbol":"tt", "total_supply":"1000000000000000000000000"}), "new_default_meta").await?;
    Ok(ft)
}
pub async fn init_nft_contract(
    worker: &TestSandbox,
    creator: &TestAccount,
) -> anyhow::Result<(TestAccount, TokenId)> {
    let nft = worker.deploy_fixture("nft", &WASMS[3]).await?;
    initialize(
        &nft,
        json!({"owner_id":creator.id(), "name":"token", "symbol":"tt"}),
        "new_default_meta",
    )
    .await?;
    let token_id = "1";
    Contract(nft.id.clone()).call_function("nft_mint", json!({"token_id":token_id, "token_owner_id":creator.id(), "token_metadata":{"copies":1, "description":"The Team Goes", "media":"https://bafybeidl4hjbpdr6u6xvlrizwxbrfcyqurzvcnn5xoilmcqbxfbdwrmp5m.ipfs.dweb.link/", "title":"GO TEAM"}}))
        .transaction()
        .deposit(NearToken::from_yoctonear(6580000000000000000000))
        .gas(near_sdk::Gas::from_tgas(100)).with_signer(creator.id.clone(),creator.signer.clone())
        .send_to(&creator.network).await?.assert_success();
    Ok((nft, token_id.to_string()))
}
