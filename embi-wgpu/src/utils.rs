use crate::*;

#[macro_export]
macro_rules! reloadable_wgsl_shader {
    ($name:literal) => {{
        let struct_prefix = include_str!("../../assets/shaders/structs.wgsl");

        cfg_if! {
            if #[cfg(feature = "ci-release")] {
                let shader = include_str!(concat!("../../assets/shaders/", $name, ".wgsl"));
            } else {
                let path = concat!("assets/shaders/", $name, ".wgsl");
                info!("DEV loading shader: {}", path);
                let shader: String = std::fs::read_to_string(path).unwrap().into();
            }
        }

        wgpu::ShaderModuleDescriptor {
            label: Some(&format!("{} Shader", $name)),
            source: wgpu::ShaderSource::Wgsl(
                format!("{}{}", struct_prefix, shader).into()
            ),
        }
    }};
}

#[macro_export]
macro_rules! reloadable_wgsl_fragment_shader {
    ($name:literal) => {{
        let struct_prefix = include_str!("../../assets/shaders/structs.wgsl");

        let pp_prefix =
            include_str!("../../assets/shaders/post_processing_vertex.wgsl");

        let frag_shader_prefix = format!("{}{}", struct_prefix, pp_prefix);

        // #[cfg(feature = "ci-release")]
        // let frag_part =
        //     include_str!(concat!("../../assets/shaders/", $name, ".wgsl"));
        //
        // #[cfg(not(feature = "ci-release"))]
        // let path = concat!("assets/shaders/", $name, ".wgsl");
        // #[cfg(not(feature = "ci-release"))]
        // info!("DEV loading shader: {}", path);
        // #[cfg(not(feature = "ci-release"))]
        // let frag_part = std::fs::read_to_string(path)
        //     .expect(&format!("shader at {path} must exist"));

        cfg_if! {
            if #[cfg(feature = "ci-release")] {
                let frag_part =
                    include_str!(concat!("../../assets/shaders/", $name, ".wgsl"));
            } else {
                let path = concat!("assets/shaders/", $name, ".wgsl");
                info!("DEV loading shader: {}", path);

                let frag_part = std::fs::read_to_string(path)
                    .expect(&format!("shader at {path} must exist"));
            }
        }

        let full_shader = format!(
            "{}{}",
            frag_shader_prefix,
            frag_part
        );

        wgpu::ShaderModuleDescriptor {
            label: Some($name),
            source: wgpu::ShaderSource::Wgsl(
                full_shader
                .into(),
            ),
        }
    }};
}

pub fn load_texture_from_engine_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    name: &str,
    bytes: &[u8],
    texture_bind_group_layout: &wgpu::BindGroupLayout,
    textures: &mut TextureMap,
    address_mode: wgpu::AddressMode,
) {
    let handle = texture_path(name);

    let img = image::load_from_memory(bytes).expect("must be valid image");
    let error_texture = Texture::from_image_ex(
        device,
        queue,
        &img,
        Some(name),
        false,
        address_mode,
    )
    .unwrap();

    let error_bind_group = device.simple_bind_group(
        &format!("{}_bind_group", name),
        &error_texture,
        texture_bind_group_layout,
    );

    ASSETS.borrow_mut().insert_handle(name, handle);
    ASSETS.borrow_mut().texture_image_map.lock().insert(handle, img);
    textures.insert(handle, (error_bind_group, error_texture));
}

pub fn simple_fragment_shader<'a>(
    name: &'static str,
    frag: &'static str,
) -> wgpu::ShaderModuleDescriptor<'a> {
    let struct_prefix = include_str!("../../assets/shaders/structs.wgsl");
    let frag_shader_prefix =
        include_str!("../../assets/shaders/post_processing_vertex.wgsl");

    wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Wgsl(
            format!("{}{}{}", struct_prefix, frag_shader_prefix, frag).into(),
        ),
    }
}

pub struct MipmapGenerator {
    pub format: wgpu::TextureFormat,
    pub blit_pipeline: wgpu::RenderPipeline,
    pub blit_layout: wgpu::BindGroupLayout,
}

impl MipmapGenerator {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let blit_pipeline = {
            let shader =
                device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Blit Shader"),
                    source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                        include_str!("../../assets/shaders/blit.wgsl"),
                    )),
                });

            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Blit Render Pipeline"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(format.into())],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };

        let blit_layout = blit_pipeline.get_bind_group_layout(0);

        Self { format, blit_pipeline, blit_layout }
    }

    pub fn generate_mipmaps(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        mip_count: u32,
    ) {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Mip Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let views = (0..mip_count)
            .map(|mip| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("Mip View"),
                    format: None,
                    dimension: None,
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    base_array_layer: 0,
                    array_layer_count: None,
                })
            })
            .collect::<Vec<_>>();

        for target_mip in 1..mip_count as usize {
            let bind_group =
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &self.blit_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &views[target_mip - 1],
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                    label: None,
                });

            let mut rpass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(
                        wgpu::RenderPassColorAttachment {
                            view: &views[target_mip],
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                                store: true,
                            },
                        },
                    )],
                    depth_stencil_attachment: None,
                });

            rpass.set_pipeline(&self.blit_pipeline);
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw(0..3, 0..1);
        }
    }
}