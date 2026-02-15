pub mod entreprise;
pub mod hidden;
pub mod psk;

use std::sync::Arc;

use crate::mode::station::auth::{
    entreprise::{
        WPAEntreprise,
        requests::{
            key_passphrase::RequestKeyPassphrase, password::RequestPassword,
            username_and_password::RequestUsernameAndPassword,
        },
    },
    hidden::HiddenSsidDialog,
    psk::Psk,
};
use crate::nm::NMClient;

#[derive(Debug, Default)]
pub struct Auth {
    pub psk: Psk,
    pub hidden: HiddenSsidDialog,
    pub eap: Option<WPAEntreprise>,
    pub request_key_passphrase: Option<RequestKeyPassphrase>,
    pub request_password: Option<RequestPassword>,
    pub request_username_and_password: Option<RequestUsernameAndPassword>,
}

impl Auth {
    pub fn init_eap(&mut self, network_name: String, client: Option<Arc<NMClient>>) {
        self.eap = Some(WPAEntreprise::new(network_name, client));
    }

    pub fn reset(&mut self) {
        self.psk = Psk::default();
        self.hidden.reset();
        self.eap = None;
    }

    pub fn init_request_key_passphrase(&mut self, network_name: String) {
        self.request_key_passphrase = Some(RequestKeyPassphrase::new(network_name));
    }

    pub fn init_request_password(&mut self, network_name: String, user_name: Option<String>) {
        self.request_password = Some(RequestPassword::new(network_name, user_name));
    }

    pub fn init_request_username_and_password(&mut self, network_name: String) {
        self.request_username_and_password = Some(RequestUsernameAndPassword::new(network_name));
    }
}
