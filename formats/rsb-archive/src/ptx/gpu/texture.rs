use super::DecodeBackend;

pub struct PtxGpuTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    backend: DecodeBackend,
}

impl PtxGpuTexture {
    pub(crate) fn new(
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        width: u32,
        height: u32,
        backend: DecodeBackend,
    ) -> Self {
        Self {
            texture,
            view,
            width,
            height,
            backend,
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub const fn backend(&self) -> DecodeBackend {
        self.backend
    }
}
