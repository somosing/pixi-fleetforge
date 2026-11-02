use futures::StreamExt;
use libp2p::{
    gossipsub, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux,
};
use std::error::Error;
use tokio::{io, io::AsyncBufReadExt, select};

#[derive(NetworkBehaviour)]
struct FleetBehaviour {
    gossipsub: gossipsub::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    let listen_port = arguments.get(1).map(String::as_str).unwrap_or("4601");
    let local_identity = libp2p::identity::Keypair::generate_ed25519();

    let gossipsub_protocol = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(local_identity.clone()),
        gossipsub::Config::default(),
    )
    .map_err(|configuration_error| std::io::Error::other(configuration_error.to_string()))?;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(local_identity)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_| FleetBehaviour {
            gossipsub: gossipsub_protocol,
        })?
        .build();

    let fleet_topic = gossipsub::IdentTopic::new("fleetforge-release-observations-v1");
    swarm.behaviour_mut().gossipsub.subscribe(&fleet_topic)?;
    swarm.listen_on(format!("/ip4/127.0.0.1/tcp/{listen_port}").parse()?)?;
    for peer_address in arguments.iter().skip(2) {
        let peer_multiaddr: libp2p::Multiaddr = peer_address.parse()?;
        swarm.dial(peer_multiaddr)?;
    }
    let mut incoming_lines = io::BufReader::new(io::stdin()).lines();
    println!("FleetForge gossip peer ready. Enter JSON release observations to publish.");
    loop {
        select! {
            line = incoming_lines.next_line() => {
                if let Some(observation) = line? {
                    if let Err(publish_error) = swarm.behaviour_mut().gossipsub.publish(fleet_topic.clone(), observation.as_bytes()) {
                        eprintln!("Publish rejected: {publish_error}");
                    }
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(FleetBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source, message, ..
                })) => {
                    println!("{propagation_source}: {}", String::from_utf8_lossy(&message.data));
                }
                SwarmEvent::NewListenAddr { address, .. } => println!("Listening: {address}"),
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    println!("Connected to {peer_id}");
                }
                _ => {}
            }
        }
    }
}
