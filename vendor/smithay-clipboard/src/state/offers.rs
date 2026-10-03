//! Bound metadata before forwarding protocol events to SCTK's offer storage.

use std::collections::HashMap;

use sctk::data_device_manager::DataDeviceManagerState;
use sctk::data_device_manager::data_device::DataDeviceData;
use sctk::data_device_manager::data_offer::DataOfferData;
use sctk::data_device_manager::data_source::DataSourceData;
use sctk::globals::GlobalData;
use sctk::reexports::client::protocol::wl_data_device::{self, WlDataDevice};
use sctk::reexports::client::protocol::wl_data_device_manager::WlDataDeviceManager;
use sctk::reexports::client::protocol::wl_data_offer::{self, WlDataOffer};
use sctk::reexports::client::protocol::wl_data_source::WlDataSource;
use sctk::reexports::client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch, event_created_child,
};
use wayland_backend::client::ObjectId;

use super::State;
use crate::rich::MAX_FORMATS;

const MAX_OFFERS: usize = 32;
const MAX_MIME_BYTES: usize = 255;

#[derive(Default)]
pub(super) struct Offers {
    active: HashMap<ObjectId, Metadata>,
}

struct Metadata {
    offer: WlDataOffer,
    count: usize,
    overflow: bool,
}

impl Offers {
    pub(super) fn valid(&self, offer: &WlDataOffer) -> bool {
        self.active.get(&offer.id()).is_some_and(|metadata| !metadata.overflow)
    }

    pub(super) fn prune(&mut self) {
        self.active.retain(|_, metadata| metadata.offer.is_alive());
    }

    fn admit(&mut self, offer: &WlDataOffer) -> bool {
        self.prune();
        if self.active.len() >= MAX_OFFERS {
            return false;
        }
        self.active
            .insert(offer.id(), Metadata { offer: offer.clone(), count: 0, overflow: false });
        true
    }

    fn mime(&mut self, offer: &WlDataOffer, mime: &str) -> bool {
        let Some(metadata) = self.active.get_mut(&offer.id()) else { return false };
        metadata.count = metadata.count.saturating_add(1);
        if metadata.count > MAX_FORMATS {
            metadata.overflow = true;
            return false;
        }
        // Unknown or unusually long consumer-specific types are simply not
        // supported. They must not prevent using another ordinary MIME format.
        !mime.is_empty() && mime.len() <= MAX_MIME_BYTES && !mime.bytes().any(|byte| byte < b' ')
    }
}

impl Dispatch<WlDataDevice, DataDeviceData, State> for State {
    event_created_child!(State, WlDataDevice, [0 => (WlDataOffer, DataOfferData::default())]);

    fn event(
        state: &mut State,
        device: &WlDataDevice,
        mut event: wl_data_device::Event,
        data: &DataDeviceData,
        connection: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        match &mut event {
            wl_data_device::Event::DataOffer { id } => {
                if !state.offers.admit(id) {
                    id.destroy();
                    return;
                }
            },
            wl_data_device::Event::Selection { id } | wl_data_device::Event::Enter { id, .. } => {
                if id.as_ref().is_some_and(|offer| !state.offers.active.contains_key(&offer.id())) {
                    *id = None;
                }
            },
            _ => {},
        }
        <DataDeviceManagerState as Dispatch<WlDataDevice, DataDeviceData, State>>::event(
            state, device, event, data, connection, queue,
        );
        state.offers.prune();
    }
}

impl Dispatch<WlDataOffer, DataOfferData, State> for State {
    fn event(
        state: &mut State,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        data: &DataOfferData,
        connection: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = &event
            && !state.offers.mime(offer, mime_type)
        {
            return;
        }
        if !state.offers.active.contains_key(&offer.id()) {
            return;
        }
        <DataDeviceManagerState as Dispatch<WlDataOffer, DataOfferData, State>>::event(
            state, offer, event, data, connection, queue,
        );
    }
}

delegate_dispatch!(State: [WlDataDeviceManager: GlobalData] => DataDeviceManagerState);
delegate_dispatch!(State: [WlDataSource: DataSourceData] => DataDeviceManagerState);
