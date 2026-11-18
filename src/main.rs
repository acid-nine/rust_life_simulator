// src/main.rs

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub speed: f32,
    pub size: f32,
    pub energy: f32,
    pub genes: AgentGenes,
}

#[derive(Debug, Clone)]
pub struct AgentGenes {
    pub speed_gene: f32,
    pub size_gene: f32,
    pub sense_range: f32,
}

pub struct World {
    pub width: f32,
    pub height: f32,
    pub agents: Vec<Agent>,
    pub food: Vec<(f32, f32)>,
}

impl World {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            agents: Vec::new(),
            food: Vec::new(),
        }
    }

    pub fn spawn_agent(&mut self, x: f32, y: f32) {
        let new_agent = Agent {
            id: rand::random(),
            x,
            y,
            speed: 1.0, // Placeholder values
            size: 1.0,
            energy: 100.0,
            genes: AgentGenes {
                speed_gene: 1.0,
                size_gene: 1.0,
                sense_range: 50.0,
            },
        };
        self.agents.push(new_agent);
    }

    pub fn step(&mut self) {
        // Logic for movement, metabolism, and food consumption goes here
    }
}

fn main() {
    let mut world = World::new(800.0, 600.0);
    println!("World initialized with {} agents.", world.agents.len());
}
