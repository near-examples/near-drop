use near_sdk::{serde_json::json, NearToken};
use near_workspaces::{Account, Contract, DevNetwork, Worker};

pub async fn init(
    worker: &Worker<impl DevNetwork>,
    root: &Account,
) -> anyhow::Result<(Contract, Account, Account)> {
    let wasm = near_workspaces::compile_project(".").await?;
    let contract = worker.dev_deploy(&wasm).await?;

    let root_contract =  near_workspaces::compile_project("./tests/contracts/root").await?;
    let _ = root.deploy(&root_contract).await?;

    let creator = root
        .create_subaccount("creator")
        .initial_balance(NearToken::from_near(2))
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
