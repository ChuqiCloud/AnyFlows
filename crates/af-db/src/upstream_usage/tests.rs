use super::*;
use sea_orm::{DbBackend,Statement};

async fn fixture()->UpstreamUsageRepository{
    let pool=crate::connect_and_migrate(&crate::DatabaseOptions::new("sqlite::memory:").unwrap(),crate::MigrationOptions::default()).await.unwrap();
    for sql in[
        "INSERT INTO groups(id,name,display_name,flags) VALUES(1,'usage','Usage','{}')",
        "INSERT INTO users(id,username,role,status,default_group_id,aff_code,settings) VALUES(1,'usage-admin',1,1,1,'usage-admin','{}')",
        "INSERT INTO channels(id,name,\"type\",protocol,status,base_url,model_mapping,param_override,header_override,settings) VALUES(1,'usage','openai','openai',1,'https://panel.example/v1','{}','{}','{}','{}')",
    ]{pool.connection().execute_unprepared(sql).await.unwrap();}
    let secret=serde_json::json!({"version":1,"algorithm":"xchacha20poly1305","key_id":"usage-test","nonce":base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD,[1_u8;24]),"ciphertext":base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD,[2_u8;32])});
    for id in[1_i64,2]{
        pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO credentials(id,channel_id,kind,secret,status,schedulable) VALUES(?,1,'api_key',?,1,1)",
            [id.into(),secret.to_string().into()])).await.unwrap();
    }
    UpstreamUsageRepository::new(pool)
}

#[tokio::test]
async fn upstream_usage_shared_profiles_reuse_source_identity_but_keep_key_pause_separate(){
    let repo=fixture().await;
    let profile=repo.save_profile(UpstreamUsageProfile{id:0,name:"Shared".to_owned(),provider:UpstreamUsageProvider::NewApi,
        base_url:"https://panel.example".to_owned(),platform_user_id:None,quota_per_unit:None,currency:None,version:0,has_secret:true},Some("encrypted".to_owned())).await.unwrap();
    let channel=ChannelId::new(1).unwrap();let key1=CredentialId::new(1).unwrap();let key2=CredentialId::new(2).unwrap();
    let config=UpstreamUsageConfig{provider:UpstreamUsageProvider::NewApi,scope:UpstreamUsageScope::Account,profile_id:Some(profile.id),pause_when_exhausted:true,..Default::default()};
    repo.save_config(UserId::new(1).unwrap(),channel,key1,config.clone()).await.unwrap();
    let mut other=config;other.pause_when_exhausted=false;
    repo.save_config(UserId::new(1).unwrap(),channel,key2,other).await.unwrap();
    let first=repo.target(channel,key1).await.unwrap();let second=repo.target(channel,key2).await.unwrap();
    assert_eq!(first.cache_key,second.cache_key);assert_eq!(first.query_identity,second.query_identity);assert_ne!(first.identity,second.identity);
    let now=now_epoch();
    let lease=repo.claim(&first,"one".to_owned(),now).await.unwrap().unwrap();
    assert!(repo.claim(&second,"two".to_owned(),now).await.unwrap().is_none());
    repo.complete(&first,lease,serde_json::json!({"remaining":"0"}),true,0,now,true,None).await.unwrap();
    assert!(repo.snapshot(&second).await.unwrap().is_some());
    let pauses=load_usage_pauses(repo.pool.connection(),&[1,2]).await.unwrap();
    assert!(pauses.contains_key(&1));assert!(!pauses.contains_key(&2));
    assert_eq!(repo.delete_profile(profile.id).await,Err(UpstreamUsageError::InUse));
    repo.save_profile(profile,None).await.unwrap();
    assert!(load_usage_pauses(repo.pool.connection(),&[1,2]).await.unwrap().is_empty());
    repo.pool.close().await.unwrap();
}

#[tokio::test]
async fn upstream_usage_channel_defaults_are_inherited_and_explicit_key_can_turn_refresh_off(){
    let repo=fixture().await;let channel=ChannelId::new(1).unwrap();let key=CredentialId::new(1).unwrap();
    let defaults=UpstreamUsageConfig{provider:UpstreamUsageProvider::NewApi,refresh_interval_secs:Some(900),..Default::default()};
    repo.save_channel_config(UserId::new(1).unwrap(),channel,defaults).await.unwrap();
    let mut key_config=repo.config(channel,key).await.unwrap();assert!(key_config.inherited);assert_eq!(key_config.version,0);assert_eq!(key_config.refresh_interval_secs,Some(900));
    key_config.refresh_interval_secs=None;
    let saved=repo.save_config(UserId::new(1).unwrap(),channel,key,key_config).await.unwrap();assert!(!saved.inherited);assert!(saved.refresh_interval_secs.is_none());
    repo.reset_config(channel,key).await.unwrap();assert_eq!(repo.config(channel,key).await.unwrap().refresh_interval_secs,Some(900));
    repo.pool.close().await.unwrap();
}
