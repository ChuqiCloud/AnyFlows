use super::*;
use sea_orm::{Statement,DbBackend};
use std::error::Error;

struct Fixture { repository:MerchantPoolRepository,pool:DatabasePool,merchant:i64,shared_pool:i64 }

async fn fixture()->Result<Fixture,Box<dyn Error>> {
    let pool=crate::connect_and_migrate(&crate::DatabaseOptions::new("sqlite::memory:")?,crate::MigrationOptions::default()).await?;
    pool.connection().execute_unprepared("INSERT INTO groups (id,name,display_name,flags) VALUES (1,'merchant-test','Merchant Test','{}')").await?;
    for (id,name) in [(1,"merchant-one"),(2,"merchant-two"),(3,"merchant-admin")] {
        pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO users (id,username,status,role,default_group_id,aff_code) VALUES (?,?,1,?,1,?)",
            [id.into(),name.into(),if id==3 {1_i16}else{0_i16}.into(),format!("merchant-aff-{id}").into()])).await?;
    }
    pool.connection().execute_unprepared("INSERT INTO model_prices (model,billing_mode,input_price,output_price) VALUES ('gpt-test',1,2,4)").await?;
    let repository=MerchantPoolRepository::new(pool.clone(),Duration::from_secs(5));
    let admin=UserId::new(3)?;
    let merchant=repository.apply(UserId::new(1)?,MerchantApplication{name:"Merchant One".to_owned(),description:"Service".to_owned()}).await?;
    repository.review_merchant(admin,merchant.id,review("approve",None)).await?;
    let shared_pool=repository.write_pool(admin,None,pool_write()).await?;
    Ok(Fixture{repository,pool,merchant:merchant.id,shared_pool:shared_pool.id})
}

fn pool_write()->MerchantPoolWrite {
    MerchantPoolWrite{group_id:1,name:"Shared Pool".to_owned(),description:"Test shared supply".to_owned(),enabled:true,
        commission_bps:900,commission_inherit_global:true,min_ratio_micros:100_000,max_ratio_micros:5_000_000,settlement_delay_secs:86_400}
}
fn service_write(pool:i64)->MerchantServiceWrite {
    MerchantServiceWrite{pool_id:pool,name:"Reviewed Service".to_owned(),description:"Reviewed description".to_owned(),
        base_url:"https://api.example.com/v1".to_owned(),protocol:"openai_chat".to_owned(),api_key:Some("upstream-private-key".to_owned()),
        default_ratio_micros:500_000,models:vec![MerchantModelWrite{model:"gpt-test".to_owned(),upstream_model:"private-upstream-model".to_owned(),
            ratio_micros:None,context_window:Some(8192),max_output_tokens:Some(2048),supports_streaming:true,supports_tools:true,supports_vision:false}],
        concurrency:Some(5),timeout_secs:60}
}
fn review(decision:&str,expected:Option<i64>)->MerchantReview {
    MerchantReview{decision:decision.to_owned(),reason:"Test administrator decision".to_owned(),expected_revision:expected}
}
fn envelope(channel:ChannelId,credential:CredentialId,_previous:Option<MerchantCredentialMaterial>,key:Option<String>)->Result<EncryptedCredentialEnvelope,MerchantPoolError> {
    assert!(channel.get()>0 && credential.get()>0);
    assert!(key.is_some());
    EncryptedCredentialEnvelope::new("merchant-test",[1;24],vec![2;32]).map_err(internal)
}

async fn publish(fixture:&Fixture)->Result<MerchantOwnedService,Box<dyn Error>> {
    let service=fixture.repository.write_service(UserId::new(1)?,None,service_write(fixture.shared_pool),envelope).await?;
    let service=fixture.repository.submit_service(UserId::new(1)?,service.id).await?;
    Ok(fixture.repository.review_service(UserId::new(3)?,service.id,review("approve",service.draft.as_ref().map(|draft|draft.revision))).await?)
}

#[tokio::test]
async fn approval_preserves_draft_isolation_and_consumer_secrets()->Result<(),Box<dyn Error>> {
    let fixture=fixture().await?;
    let service=fixture.repository.write_service(UserId::new(1)?,None,service_write(fixture.shared_pool),envelope).await?;
    assert!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?.is_empty());
    fixture.repository.submit_service(UserId::new(1)?,service.id).await?;
    assert!(matches!(fixture.repository.review_service(UserId::new(3)?,service.id,review("approve",Some(99))).await,Err(MerchantPoolError::Conflict)));
    let published=fixture.repository.review_service(UserId::new(3)?,service.id,review("approve",Some(1))).await?;
    assert_eq!(published.status,"published");
    let candidates=fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?;
    assert_eq!(candidates.len(),1);assert_eq!(candidates[0].sales_ratio_micros,500_000);
    let catalog=fixture.repository.catalog(UserId::new(2)?,fixture.shared_pool,MerchantListQuery::default()).await?;
    assert_eq!(catalog.items[0].models[0].prices.as_ref().expect("priced model").input,"1");
    let encoded=serde_json::to_string(&catalog)?;
    for secret in ["api.example.com","private-upstream-model","upstream-private-key","credential_id","channel_id"] { assert!(!encoded.contains(secret)); }
    let mut draft=service_write(fixture.shared_pool);draft.name="Unapproved title".to_owned();draft.default_ratio_micros=1_000_000;
    fixture.repository.write_service(UserId::new(1)?,Some(service.id),draft,envelope).await?;
    let catalog=fixture.repository.catalog(UserId::new(2)?,fixture.shared_pool,MerchantListQuery::default()).await?;
    assert_eq!(catalog.items[0].name,"Reviewed Service");
    assert_eq!(catalog.items[0].models[0].sales_ratio_micros,500_000);
    fixture.pool.close().await?;Ok(())
}

#[tokio::test]
async fn failed_credential_encryption_rolls_back_all_draft_resources()->Result<(),Box<dyn Error>> {
    let fixture=fixture().await?;
    let result=fixture.repository.write_service(UserId::new(1)?,None,service_write(fixture.shared_pool),|_,_,_,_|Err(MerchantPoolError::Unavailable)).await;
    assert!(matches!(result,Err(MerchantPoolError::Unavailable)));
    assert_eq!(services::Entity::find().count(fixture.pool.connection()).await?,0);
    assert_eq!(channels::Entity::find().count(fixture.pool.connection()).await?,0);
    assert_eq!(credentials::Entity::find().count(fixture.pool.connection()).await?,0);
    fixture.pool.close().await?;Ok(())
}

#[tokio::test]
async fn owner_cannot_read_or_pause_another_merchants_service()->Result<(),Box<dyn Error>> {
    let fixture=fixture().await?;let service=publish(&fixture).await?;
    let other=fixture.repository.apply(UserId::new(2)?,MerchantApplication{name:"Merchant Two".to_owned(),description:String::new()}).await?;
    fixture.repository.review_merchant(UserId::new(3)?,other.id,review("approve",None)).await?;
    assert!(matches!(fixture.repository.get_service(Some(UserId::new(2)?),service.id).await,Err(MerchantPoolError::NotFound)));
    assert!(matches!(fixture.repository.pause_service(UserId::new(2)?,service.id,true).await,Err(MerchantPoolError::NotFound)));
    assert!(matches!(fixture.repository.write_service(UserId::new(2)?,Some(service.id),service_write(fixture.shared_pool),envelope).await,Err(MerchantPoolError::NotFound)));
    fixture.pool.close().await?;Ok(())
}

#[tokio::test]
async fn platform_suspension_cannot_be_undone_by_merchant_resume()->Result<(),Box<dyn Error>> {
    let fixture=fixture().await?;let service=publish(&fixture).await?;
    fixture.repository.pause_service(UserId::new(1)?,service.id,true).await?;
    fixture.repository.review_service(UserId::new(3)?,service.id,review("suspend",Some(1))).await?;
    assert!(matches!(fixture.repository.pause_service(UserId::new(1)?,service.id,false).await,Err(MerchantPoolError::Conflict)));
    let service=fixture.repository.review_service(UserId::new(3)?,service.id,review("approve",Some(1))).await?;
    assert_eq!(service.status,"paused");
    assert!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?.is_empty());
    fixture.repository.pause_service(UserId::new(1)?,service.id,false).await?;
    assert_eq!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?.len(),1);
    fixture.pool.close().await?;Ok(())
}

#[tokio::test]
async fn merchant_commission_overrides_pool_and_global_and_is_resolved_for_owner()->Result<(),Box<dyn Error>> {
    let fixture=fixture().await?;publish(&fixture).await?;
    let admin=UserId::new(3)?;
    fixture.repository.write_settings(admin,MerchantPoolSettings{commission_bps:650}).await?;
    assert_eq!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?[0].commission_bps,650);
    fixture.repository.write_terms(admin,fixture.shared_pool,fixture.merchant,MerchantPoolTerms{commission_bps:Some(200)}).await?;
    assert_eq!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?[0].commission_bps,200);
    assert_eq!(fixture.repository.available_supply_pools(UserId::new(1)?,MerchantListQuery::default()).await?.items[0].effective_commission_bps,200);
    fixture.repository.write_terms(admin,fixture.shared_pool,fixture.merchant,MerchantPoolTerms{commission_bps:None}).await?;
    let mut config=pool_write();config.commission_inherit_global=false;
    fixture.repository.write_pool(admin,Some(fixture.shared_pool),config).await?;
    assert_eq!(fixture.repository.published_candidates(fixture.shared_pool,None,"gpt-test").await?[0].commission_bps,900);
    fixture.pool.close().await?;Ok(())
}
