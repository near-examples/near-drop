use near_contract_standards::non_fungible_token::TokenId;
use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{Account, Contract, DevNetwork, Worker};

pub async fn init(
    worker: &Worker<impl DevNetwork>,
    root: &Account,
) -> anyhow::Result<(Account, Account, Account)> {
    let root_wasm = near_workspaces::compile_project("./tests/contracts/root").await?;
    let _ = root.deploy(&root_wasm).await?;

    let contract = root
        .create_subaccount("contract")
        .initial_balance(NearToken::from_yoctonear(182620304538970019274880u128 + 1418524372904400000000u128))
        .transact()
        .await?
        .unwrap();

    let wasm = near_workspaces::compile_project(".").await?;
    contract.deploy(&wasm).await?;

    let creator = root
        .create_subaccount("creator")
        .initial_balance(NearToken::from_near(5))
        .transact()
        .await?
        .unwrap();
    let alice = root.create_subaccount("alice").transact().await?.unwrap();

    let res = contract
        .call(contract.id(), "new")
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
    let ft_wasm = near_workspaces::compile_project("./tests/contracts/ft").await?;
    let ft_contract = worker.dev_deploy(&ft_wasm).await?;

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
    let nft_wasm = near_workspaces::compile_project("./tests/contracts/nft").await?;
    let nft_contract = worker.dev_deploy(&nft_wasm).await?;

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
