//! This module implements a basic relay for VMBus messages between the host and guest.
use futures::future::poll_fn;
use hcl::vmbus::HclVmbus;
use hvdef::Vtl;
use pal_async::task::Spawn;
use pal_async::task::Task;
use std::sync::Arc;
use std::task::Poll;
use std::thread;
use std::u32;
use vmbus_async::async_dgram::AsyncRecvExt;
use vmbus_client::VmbusMessageSource;
use vmbus_core::OutgoingMessage;
use vmbus_core::VersionInfo;
use vmbus_core::protocol;
use vmcore::synic::GuestMessagePort;
use vmcore::synic::MessagePort;
use vmcore::synic::SynicPortAccess;

/// A simple relay for VMBus messages that connects the host message source to the guest message port.
pub struct VmbusSimpleRelay {
    _task: Task<RelayTask>,
}

/// Implements the `VmbusSimpleRelay`
impl VmbusSimpleRelay {
    /// Starts the relay task.
    pub fn start(
        spawner: &impl Spawn,
        host_msg_source: Box<dyn VmbusMessageSource>,
        hcl_vmbus: Arc<HclVmbus>,
        synic: Arc<dyn SynicPortAccess>,
    ) -> Self {
        let mut relay_task = RelayTask::new(host_msg_source, hcl_vmbus, synic);
        let task = spawner.spawn("vmbus simple relay", async move {
            relay_task.run().await;
            relay_task
        });
        Self { _task: task }
    }
}

struct RelayTask {
    host_msg_source: Box<dyn VmbusMessageSource>,
    _message_port: Box<dyn Sync + Send>,
    _mp_message_port: Box<dyn Sync + Send>,
    guest_message_port: Box<dyn GuestMessagePort>,
}

impl RelayTask {
    /// Creates a new relay task that connects the host message source to the guest message port.
    pub fn new(
        host_msg_source: Box<dyn VmbusMessageSource>,
        hcl_vmbus: Arc<HclVmbus>,
        synic: Arc<dyn SynicPortAccess>,
    ) -> Self {
        let port = RelayMessagePort {
            hcl_vmbus: Arc::clone(&hcl_vmbus),
        };
        let message_port = synic
            .add_message_port(1, Vtl::Vtl0, Arc::new(port))
            .unwrap();

        let port = RelayMessagePort {
            hcl_vmbus: Arc::clone(&hcl_vmbus),
        };
        let _mp_message_port = synic
            .add_message_port(4, Vtl::Vtl0, Arc::new(port))
            .unwrap();
        let guest_message_port = synic.new_guest_message_port(Vtl::Vtl0, 0, 2).unwrap();

        Self {
            host_msg_source,
            _message_port: message_port,
            _mp_message_port,
            guest_message_port,
        }
    }

    /// Starts the relay task.
    pub async fn run(&mut self) {
        let mut buf = [0; protocol::MAX_MESSAGE_SIZE];
        self.host_msg_source.resume_message_stream();
        const VERSION: VersionInfo = VersionInfo {
            version: protocol::Version::Copper,
            feature_flags: protocol::FeatureFlags::from_bits(u32::MAX),
        };
        loop {
            let size = match self.host_msg_source.recv(&mut buf).await {
                Ok(size) => size,
                Err(e) => {
                    tracing::error!(?e, "Failed to receive message");
                    continue;
                }
            };

            if size == 0 {
                tracing::warn!("End of pipe");
                break;
            }

            let msg = &buf[..size];
            if let Ok(parsed) = protocol::Message::parse(msg, Some(VERSION)) {
                tracing::info!(?parsed, "host message");
                if let protocol::Message::VersionResponse2(mut response, _) = parsed {
                    response.version_response.selected_version_or_connection_id = 1;
                    poll_fn(|cx| {
                        self.guest_message_port.poll_post_message(
                            cx,
                            1,
                            OutgoingMessage::new(&response).data(),
                        )
                    })
                    .await;
                    continue;
                }

                if let protocol::Message::VersionResponse(mut response, _) = parsed {
                    response.selected_version_or_connection_id = 1;
                    poll_fn(|cx| {
                        self.guest_message_port.poll_post_message(
                            cx,
                            1,
                            OutgoingMessage::new(&response).data(),
                        )
                    })
                    .await;
                    continue;
                }
            }
            poll_fn(|cx| {
                self.guest_message_port
                    .poll_post_message(cx, 1, &buf[..size])
            })
            .await;
        }
    }
}

struct RelayMessagePort {
    hcl_vmbus: Arc<HclVmbus>,
}

impl MessagePort for RelayMessagePort {
    fn poll_handle_message(
        &self,
        _cx: &mut std::task::Context<'_>,
        msg: &[u8],
        _trusted: bool,
    ) -> Poll<()> {
        const VERSION: VersionInfo = VersionInfo {
            version: protocol::Version::Copper,
            feature_flags: protocol::FeatureFlags::from_bits(u32::MAX),
        };
        if let Ok(parsed) = protocol::Message::parse(msg, Some(VERSION)) {
            tracing::info!(?parsed, "guest message");
            if let protocol::Message::InitiateContact2(mut ic, _) = parsed {
                let mut target_info = protocol::TargetInfo::from_bits(
                    ic.initiate_contact.interrupt_page_or_target_info,
                );
                if target_info.sint() == 0 {
                    tracing::info!("Fixing SINT");
                    target_info.set_sint(2);
                    ic.initiate_contact.interrupt_page_or_target_info = target_info.into_bits();
                    if let Err(e) = self.hcl_vmbus.post_message(
                        protocol::VMBUS_MESSAGE_REDIRECT_CONNECTION_ID,
                        1,
                        OutgoingMessage::new(&ic).data(),
                    ) {
                        tracing::error!(?e, "Failed to post message");
                    }

                    return Poll::Ready(());
                }
            }

            if let protocol::Message::InitiateContact(mut ic, _) = parsed {
                let mut target_info =
                    protocol::TargetInfo::from_bits(ic.interrupt_page_or_target_info);
                if target_info.sint() == 0 {
                    tracing::info!("Fixing SINT");
                    target_info.set_sint(2);
                    ic.interrupt_page_or_target_info = target_info.into_bits();
                    if let Err(e) = self.hcl_vmbus.post_message(
                        protocol::VMBUS_MESSAGE_REDIRECT_CONNECTION_ID,
                        1,
                        OutgoingMessage::new(&ic).data(),
                    ) {
                        tracing::error!(?e, "Failed to post message");
                    }

                    return Poll::Ready(());
                }
            }
        }
        if let Err(e) =
            self.hcl_vmbus
                .post_message(protocol::VMBUS_MESSAGE_REDIRECT_CONNECTION_ID, 1, msg)
        {
            tracing::error!(?e, "Failed to post message");
            return Poll::Pending;
        }

        Poll::Ready(())
    }
}
