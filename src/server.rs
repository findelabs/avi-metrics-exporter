use hyper::{Body, Method, Request, Response, StatusCode};
use std::error::Error;
use clap::ArgMatches;
use std::time::Duration;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, ACCEPT_ENCODING, CONTENT_TYPE};
use std::sync::RwLock;
use chrono::offset::Utc;
use chrono::NaiveDateTime;
use chrono::DateTime;
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use tokio::sync::Semaphore;

type BoxResult<T> = Result<T,Box<dyn Error + Send + Sync>>;

pub type Config = Arc<RwLock<HashMap<String, ConfigEntry>>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigEntry {
    #[serde(default)]
    pub entity_name: Vec<String>,
    #[serde(default)]
    pub tenant: Vec<String>,
    #[serde(default)]
    pub metric_id: Vec<String>,
    #[serde(default)]
    pub description: bool
}

#[derive(Debug, Clone)]
pub struct AviClient {
    client: reqwest::Client,
    expires: i64,
    threads: u16,
    username: String,
    password: String,
    controller: String,
    config_path: String,
    config: Config
}

// This is the main handler, to catch any failures in the echo fn
pub async fn main_handler(
    req: Request<Body>,
    client: AviClient
) -> BoxResult<Response<Body>> {
    match echo(req, client).await {
        Ok(s) => {
            log::debug!("Handler got success");
            Ok(s)
        }
        Err(e) => {
            log::error!("Handler caught error: {}", e);
            let mut response = Response::new(Body::from(format!("{{\"error\" : \"{}\"}}", e)));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            Ok(response)
        }
    }
}

// This is our service handler. It receives a Request, routes on its
// path, and returns a Future of a Response.
async fn echo(req: Request<Body>, mut client: AviClient) -> BoxResult<Response<Body>> {
    // Get path
    let path = &req.uri().path();

    match (req.method(), path) {
        (&Method::GET, &"/metrics") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let mut response = Response::new(Body::from(client.metrics().await?));
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        },
        (&Method::GET, &"/login") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let mut response = Response::new(Body::from(client.login().await?));
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        },
        (&Method::GET, &"/expires") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let mut response = Response::new(Body::from(client.expires()));
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        },
        (&Method::GET, &"/config") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let mut response = Response::new(Body::from(client.config().await?));
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        },
        (&Method::GET, &"/refresh_config") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let mut response = Response::new(Body::from(client.refresh_config().await?));
            *response.status_mut() = StatusCode::OK;
            Ok(response)
        },
        (&Method::GET, &"/health") => {
            let path = req.uri().path();
            log::info!("Received GET to {}", &path);
            let string = client.health().await?;
            let mut response = Response::new(Body::from(string.clone()));
            match string.as_str() {
                "Healthy" => *response.status_mut() = StatusCode::OK,
                _ => *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE
            };
            Ok(response)
        },
        _ => Ok(Response::new(Body::from(format!(
            "{{ \"msg\" : \"{} {} is not a recognized action\" }}",
            req.method(),
            path)
        ))),
    }
}

impl AviClient {
    pub async fn new(opts: ArgMatches<'_>) -> BoxResult<Self> {

        let config = AviClient::get_config(&opts.value_of("config").unwrap())?;

        let client = reqwest::Client::builder()
            .timeout(Duration::new(60, 0))
            .cookie_store(true)
            .danger_accept_invalid_certs(opts.value_of("accept_invalid").unwrap().parse()?)
            .build()
            .expect("Failed to build client");

        // Get username, password, and data
        let username = opts.value_of("username").unwrap().to_string();
        let password = opts.value_of("password").unwrap().to_string();
        let controller = opts.value_of("controller").unwrap().to_string();
        let config_path = opts.value_of("config").unwrap().to_string();
        let data = format!("{{\"username\": \"{}\", \"password\": \"{}\"}}", username.clone(), password.clone());
        let uri = format!("https://{}/login", controller.clone());

        let threads: u16 = opts.value_of("threads").unwrap().parse().unwrap_or_else(|_| {
            log::error!("specified thread count not in a valid range, defaulting to 4");
            4
        });
    
        let response = client
            .post(uri)
            .headers(AviClient::headers().await?)
            .body(data)
            .send()
            .await?;

        // Will need to handle bad logins somehow
        let max_age = match response.cookies().find(|x| x.name() == "avi-sessionid").map(|x| (x.value().to_string(), x.max_age().unwrap().as_secs())) {
            Some(e) => {
                log::debug!("Got back csrf token of: {}", e.0);
                e.1
            },
            None => 0u64
        };

        let expires = Utc::now().timestamp() + max_age as i64;
        Ok(Self { client, expires, threads, username, password, controller, config_path, config: Arc::new(RwLock::new(config)) })

    }

    pub async fn login(&mut self) -> BoxResult<String> {
        let data = format!("{{\"username\": \"{}\", \"password\": \"{}\"}}", self.username, self.password);
        let uri = format!("https://{}/login", self.controller.clone());
    
        let response = self.client
            .post(uri)
            .headers(AviClient::headers().await?)
            .body(data)
            .send()
            .await?;

        // Will need to handle bad logins somehow
        let expires = match response.cookies().find(|x| x.name() == "avi-sessionid").map(|x| x.max_age().unwrap().as_secs()) {
            Some(e) => {
                log::info!("Picked up new csrf token");

                // Update max_age for new token
                self.expires = Utc::now().timestamp() + e as i64;
                self.expires
            },
            None => {
                log::info!("Failed getting csrf token");
                self.expires = 0;
                self.expires
            }
        };
        Ok(format!("Login expires in {}", expires))
    }

    pub async fn headers() -> BoxResult<HeaderMap> {
        // Create HeaderMap
        let mut headers = HeaderMap::new();

        // Add all headers
        headers.insert("X-Avi-Version", HeaderValue::from_str("18.1.2").unwrap());
        headers.insert(USER_AGENT, HeaderValue::from_str("kraken-rs").unwrap());
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str("application/json").unwrap(),
        );
        headers.insert(
            ACCEPT_ENCODING,
            HeaderValue::from_str("application/json").unwrap(),
        );

        // Return headers
        Ok(headers)
    }

    // Return back the time in UTC that the cookie will expire
    pub fn expires(&self) -> String {
        let naive = NaiveDateTime::from_timestamp(self.expires, 0);
        let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);
        let newdate = datetime.format("%Y-%m-%d %H:%M:%S");
        newdate.to_string()
    }

    async fn renew(&mut self) -> BoxResult<()> {
        if self.expires - Utc::now().timestamp() <= 0 {
            log::info!("renew function kicking off re-login function");
            self.login().await?;
        }
        Ok(())
    }

    async fn health(&self) -> BoxResult<String> {
        let results = match self.expires {
            0 => "Not Healthy".to_string(),
            _ => "Healthy".to_string()
        };
       
        Ok(results)
    }

    async fn config(&self) -> BoxResult<String> {
        let config = self.config.read().expect("Failed to get config");

        let output = format!("{:#?}", config);
        Ok(output)
    }

    pub async fn metrics(&mut self) -> BoxResult<String> {
        // Renew creds if required
        self.renew().await?;

        // Get config
        let config = self.config.read().expect("Could not get config").clone();

        // Create string buffer
        let string = Arc::new(Mutex::new(String::new()));

        // Create vector for task handles
        let mut handles = vec![];

        // Let's rate limit to just 4 gets at once
        let sem = Arc::new(Semaphore::new(self.threads.into()));

        // Loop over each path in config
        for (path, entry) in config {

            // If tenant is empty, create blank entry
            let tenants = match &entry.tenant.len() {
                0 => vec!["empty".to_string()],
                _ => entry.tenant.clone()
            };
            
            // Loop over each tenant
            for tenant in tenants {

                // Get permission to kick off task
                let permit = Arc::clone(&sem).acquire_owned().await;

                // Get clones of all our vars, to pass into thread, these are all lightweight since all are wrapped in Arc's, or are small strings
                let string_clone = string.clone();
                let path = path.clone();
                let entry = entry.clone();
                let me = self.clone();

                handles.push(tokio::spawn(async move {
                    let _permit = permit;

                    log::info!("Getting {} for tenant {}", &path.clone(), &tenant);

                    // Declare new uri
                    let uri = format!("https://{}{}", me.controller, path);

                    // Create new vec for queries
                    let mut queries = Vec::new();
                    
                    if &entry.tenant.len() > &0 {
                        queries.push(("tenant", entry.tenant.join(",").clone()));
                    };

                    if &entry.entity_name.len() > &0 {
                        queries.push(("entity_name", entry.entity_name.join(",").clone()));
                    };

                    if &entry.metric_id.len() > &0 {
                        queries.push(("metric_id", entry.metric_id.join(",").clone()));
                    };

                    if &entry.description == &true {
                        queries.push(("description", "true".to_owned()));
                    };

                    let response = match me.get(&uri, queries).await {
                        Ok(r) => r,
                        Err(e) => {
                            log::error!("Failed to get body: {}", e);
                            String::new()
                        }
                    };

                    string_clone.lock().expect("Could not lock mutex").push_str(&response);
                    log::info!("Finished getting {} for tenant {}", &path.clone(), &tenant);
                }));
            }
        }

        log::info!("Waiting for all futures to complete");
        futures::future::join_all(handles).await;
        log::info!("All futures have completed");

        let results = string.lock().expect("wow this failed");

        Ok(results.to_string())
    }

    pub async fn get(&self, uri: &str, queries: Vec<(&str, String)>) -> BoxResult<String> {
        let response = self.client
            .get(uri)
            .query(&queries)
            .headers(AviClient::headers().await?)
            .send()
            .await?
            .text()
            .await?;

        Ok(response)
    }

    pub async fn refresh_config(&self) -> BoxResult<String> {
        self.update_config().await?;
        self.config().await
    }

    pub async fn update_config(&self) -> BoxResult<()> {

        let new_config = AviClient::get_config(&self.config_path)?;
        let mut config = self.config.write().expect("Failed to get config");
        *config = new_config;

        Ok(())
    }

    pub fn get_config(path: &str) -> Result<HashMap<String, ConfigEntry>, serde_yaml::Error> {
        let mut file = File::open(path).expect("Unable to open config");
        let mut contents = String::new();
    
        file.read_to_string(&mut contents)
            .expect("Unable to read config");
    
        let deck: HashMap<String, ConfigEntry> = serde_yaml::from_str(&contents)?;
    
        Ok(deck)
    }
}

