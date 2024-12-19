use std::sync::LazyLock;

use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{Account, Contract, DevNetwork, Worker};

static ROOT_CONTRACT_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let artifact_path = "tests/contracts/root/res/root.wasm";

    let contract_wasm = std::fs::read(artifact_path).expect(
        format!(
            "Could not read Root contract WASM file from {}",
            artifact_path
        )
        .as_str(),
    );

    contract_wasm
});

static FT_CONTRACT_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let artifact_path = "tests/contracts/ft/res/fungible_token.wasm";

    let contract_wasm = std::fs::read(artifact_path).expect(
        format!(
            "Could not read Fungible token WASM file from {}",
            artifact_path
        )
        .as_str(),
    );

    contract_wasm
});

static NFT_CONTRACT_WASM: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let artifact_path = "tests/contracts/nft/res/non_fungible_token.wasm";

    let contract_wasm = std::fs::read(artifact_path).expect(
        format!(
            "Could not read Non-fungible token WASM file from {}",
            artifact_path
        )
        .as_str(),
    );

    contract_wasm
});

pub async fn init(
    worker: &Worker<impl DevNetwork>,
    root: &Account,
) -> anyhow::Result<(Contract, Account, Account)> {
    let wasm = near_workspaces::compile_project(".").await?;
    let contract = worker.dev_deploy(&wasm).await?;

    let _ = root.deploy(&ROOT_CONTRACT_WASM).await?;

    let creator = root
        .create_subaccount("creator")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .unwrap();
    let alice = root.create_subaccount("alice").transact().await?.unwrap();

    let res = contract
        .call("new")
        .args_json(json!({"top_level_account": root.id()}))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok((contract, creator, alice));
}

pub async fn init_ft_contract(
    worker: &Worker<impl DevNetwork>,
    creator: &Account,
) -> anyhow::Result<Contract> {
    let ft_contract = worker.dev_deploy(&FT_CONTRACT_WASM).await?;

    let res = ft_contract
        .call("new_default_meta")
        .args_json(json!({"owner_id": creator.id(), "name": "token", "symbol": "tt", "total_supply": "1000000000000000000000000" }))
        .max_gas()
        .transact()
        .await?;
    assert!(res.is_success());

    return Ok(ft_contract);
}

pub async fn init_nft_contract(
    worker: &Worker<impl DevNetwork>,
    creator: &Account,
) -> anyhow::Result<(Contract, TokenId)> {
    let nft_contract = worker.dev_deploy(&NFT_CONTRACT_WASM).await?;

    let new_default_res = nft_contract
        .call("new_default_meta")
        .args_json(json!({"owner_id": creator.id(), "name": "token", "symbol": "tt" }))
        .max_gas()
        .transact()
        .await?;
    assert!(new_default_res.is_success());

    let token_id = "1";
    let mint_res = creator
        .call(nft_contract.id(), "nft_mint")
        .args_json(json!({"token_id": token_id, "token_owner_id": creator.id(), "token_metadata": {"copies": 1, "description": "The Team Goes", "media": "https://bafybeidl4hjbpdr6u6xvlrizwxbrfcyqurzvcnn5xoilmcqbxfbdwrmp5m.ipfs.dweb.link/", "title": "GO TEAM"}}))
        .deposit(NearToken::from_yoctonear(6580000000000000000000))
        .max_gas()
        .transact()
        .await?;
    assert!(mint_res.is_success());

    return Ok((nft_contract, token_id.to_string()));
}
