//! Draws a transition: keeps a copy of the frame last shown, and while a transition runs, mixes that with the
//! newly drawn frame on the back buffer. Only exists while a transition effect is chosen; the two extra copies a
//! running transition needs last only as long as it does.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

const VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dissolve_vs.cso"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dissolve_ps.cso"));

/// One frame of a transition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dissolve {
    /// The pattern, by its number in the shader.
    pub effect: u32,
    /// Left, top, right, bottom, in pixels.
    pub rect: [u32; 4],
    /// 0 shows the frame before, 1 the frame after.
    pub progress: f32,
    /// The side of the pattern's cells, in pixels.
    pub cell: f32,
    /// It's just started: the frame before is whatever was last shown.
    pub restart: bool,
}

/// The shader's constants.
#[repr(C)]
struct Constants {
    origin: [f32; 2],
    size: [f32; 2],
    progress: f32,
    effect: u32,
    cell: f32,
    padding: f32,
}

/// A copy of a frame.
struct Copy {
    texture: ID3D11Texture2D,
    view: ID3D11ShaderResourceView,
}

impl Copy {
    fn new(device: &ID3D11Device, [width, height]: [u32; 2]) -> windows::core::Result<Self> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let mut texture = None;
        let mut view = None;
        unsafe {
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            device.CreateShaderResourceView(texture.as_ref().unwrap(), None, Some(&mut view))?;
        }
        Ok(Self { texture: texture.unwrap(), view: view.unwrap() })
    }

    fn take_from(&self, context: &ID3D11DeviceContext, source: &ID3D11Texture2D) {
        unsafe { context.CopyResource(&self.texture, source) };
    }
}

pub struct Dissolver {
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    rasterizer: ID3D11RasterizerState,
    constants: ID3D11Buffer,
    /// The frame last shown.
    shown: Option<Copy>,
    /// While a transition runs: the frame it started from, and the frame just drawn.
    before: Option<Copy>,
    after: Option<Copy>,
}

impl Dissolver {
    pub fn new(device: &ID3D11Device) -> windows::core::Result<Self> {
        let mut vertex_shader = None;
        let mut pixel_shader = None;
        let mut rasterizer = None;
        let mut constants = None;
        unsafe {
            device.CreateVertexShader(VERTEX_SHADER, None, Some(&mut vertex_shader))?;
            device.CreatePixelShader(PIXEL_SHADER, None, Some(&mut pixel_shader))?;
            let rasterizer_desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                ScissorEnable: true.into(),
                DepthClipEnable: false.into(),
                ..Default::default()
            };
            device.CreateRasterizerState(&rasterizer_desc, Some(&mut rasterizer))?;
            let constants_desc = D3D11_BUFFER_DESC {
                ByteWidth: size_of::<Constants>() as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            device.CreateBuffer(&constants_desc, None, Some(&mut constants))?;
        }
        Ok(Self {
            vertex_shader: vertex_shader.unwrap(),
            pixel_shader: pixel_shader.unwrap(),
            rasterizer: rasterizer.unwrap(),
            constants: constants.unwrap(),
            shown: None,
            before: None,
            after: None,
        })
    }

    /// The window changed size, so the copies no longer fit. A running transition finishes without showing.
    pub fn forget(&mut self) {
        self.shown = None;
        self.before = None;
        self.after = None;
    }

    /// With the frame drawn on the back buffer and not yet presented: mixes in the frame before, if a
    /// transition is running, and keeps a copy of what's about to be shown.
    pub fn end_frame(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        back: &ID3D11Texture2D,
        target: &ID3D11RenderTargetView,
        size: [u32; 2],
        dissolve: Option<Dissolve>,
    ) -> windows::core::Result<()> {
        match dissolve {
            Some(d) => {
                // Starting again midway carries on from the mix on screen, so nothing jumps.
                if d.restart {
                    std::mem::swap(&mut self.before, &mut self.shown);
                }
                if let Some(before) = &self.before {
                    let after = match self.after.take() {
                        Some(after) => after,
                        None => Copy::new(device, size)?,
                    };
                    after.take_from(context, back);
                    self.mix(context, target, &before.view, &after.view, size, d)?;
                    self.after = Some(after);
                }
            }
            None => {
                self.before = None;
                self.after = None;
            }
        }
        let shown = match self.shown.take() {
            Some(shown) => shown,
            None => Copy::new(device, size)?,
        };
        shown.take_from(context, back);
        self.shown = Some(shown);
        Ok(())
    }

    fn mix(
        &self,
        context: &ID3D11DeviceContext,
        target: &ID3D11RenderTargetView,
        before: &ID3D11ShaderResourceView,
        after: &ID3D11ShaderResourceView,
        [width, height]: [u32; 2],
        d: Dissolve,
    ) -> windows::core::Result<()> {
        let [left, top, right, bottom] = d.rect;
        let constants = Constants {
            origin: [left as f32, top as f32],
            size: [right.saturating_sub(left).max(1) as f32, bottom.saturating_sub(top).max(1) as f32],
            progress: d.progress,
            effect: d.effect,
            cell: d.cell,
            padding: 0.0,
        };
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&self.constants, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            std::ptr::write(mapped.pData as *mut Constants, constants);
            context.Unmap(&self.constants, 0);

            context.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            context.OMSetBlendState(None, None, u32::MAX);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT { TopLeftX: 0.0, TopLeftY: 0.0, Width: width as f32, Height: height as f32, MinDepth: 0.0, MaxDepth: 1.0 }]));
            context.RSSetState(&self.rasterizer);
            context.RSSetScissorRects(Some(&[RECT { left: left as i32, top: top as i32, right: right as i32, bottom: bottom as i32 }]));
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetInputLayout(None);
            context.VSSetShader(&self.vertex_shader, None);
            context.PSSetShader(&self.pixel_shader, None);
            context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(before.clone()), Some(after.clone())]));
            context.Draw(3, 0);
            // Free to be copied into again next frame.
            context.PSSetShaderResources(0, Some(&[None, None]));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;

    const SIZE: [u32; 2] = [64, 48];
    const OLD: [u8; 4] = [200, 40, 10, 255];
    const NEW: [u8; 4] = [10, 90, 220, 255];
    /// The same in both frames, in the second row of each.
    const SAME: [u8; 4] = [123, 45, 67, 255];

    struct Rig {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        target: ID3D11Texture2D,
        view: ID3D11RenderTargetView,
        staging: ID3D11Texture2D,
        dissolver: Dissolver,
    }

    impl Rig {
        /// On Windows' software renderer, which every machine has.
        fn new() -> Rig {
            let (mut device, mut context) = (None, None);
            unsafe {
                D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context)).unwrap();
            }
            let (device, context): (ID3D11Device, ID3D11DeviceContext) = (device.unwrap(), context.unwrap());
            let mut desc = D3D11_TEXTURE2D_DESC {
                Width: SIZE[0],
                Height: SIZE[1],
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let (mut target, mut view, mut staging) = (None, None, None);
            unsafe {
                device.CreateTexture2D(&desc, None, Some(&mut target)).unwrap();
                device.CreateRenderTargetView(target.as_ref().unwrap(), None, Some(&mut view)).unwrap();
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                device.CreateTexture2D(&desc, None, Some(&mut staging)).unwrap();
            }
            let dissolver = Dissolver::new(&device).unwrap();
            Rig { device, context, target: target.unwrap(), view: view.unwrap(), staging: staging.unwrap(), dissolver }
        }

        /// "Draws" a frame of one colour (with row 1 the colour both frames share), then ends it.
        fn frame(&mut self, colour: [u8; 4], dissolve: Option<Dissolve>) -> Vec<[u8; 4]> {
            let mut pixels = vec![colour; (SIZE[0] * SIZE[1]) as usize];
            pixels[SIZE[0] as usize..2 * SIZE[0] as usize].fill(SAME);
            unsafe { self.context.UpdateSubresource(&self.target, 0, None, pixels.as_ptr().cast(), SIZE[0] * 4, 0) };
            self.dissolver.end_frame(&self.device, &self.context, &self.target, &self.view, SIZE, dissolve).unwrap();
            self.read()
        }

        fn read(&self) -> Vec<[u8; 4]> {
            let mut out = Vec::new();
            unsafe {
                self.context.CopyResource(&self.staging, &self.target);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
                for y in 0..SIZE[1] as usize {
                    let row = (mapped.pData as *const u8).add(y * mapped.RowPitch as usize) as *const [u8; 4];
                    out.extend_from_slice(std::slice::from_raw_parts(row, SIZE[0] as usize));
                }
                self.context.Unmap(&self.staging, 0);
            }
            out
        }
    }

    fn step(effect: u32, progress: f32, restart: bool) -> Option<Dissolve> {
        Some(Dissolve { effect, rect: [0, 0, SIZE[0], SIZE[1]], progress, cell: 8.0, restart })
    }

    fn near(a: [u8; 4], b: [u8; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
    }

    /// Every pixel but the row both frames share.
    fn differing(frame: &[[u8; 4]]) -> impl Iterator<Item = [u8; 4]> + '_ {
        frame.iter().enumerate().filter(|(i, _)| i / SIZE[0] as usize != 1).map(|(_, p)| *p)
    }

    #[test]
    fn every_effect_runs_from_the_old_frame_to_the_new_one() {
        for effect in 1..=3 {
            let mut rig = Rig::new();
            rig.frame(OLD, None);
            assert!(differing(&rig.frame(NEW, step(effect, 0.0, true))).all(|p| near(p, OLD)), "effect {effect} starts on the old frame");
            let middle = rig.frame(NEW, step(effect, 0.5, false));
            let touched = differing(&middle).filter(|p| !near(*p, OLD)).count();
            assert!(touched > 0, "effect {effect} has begun by halfway");
            assert!(differing(&middle).any(|p| !near(p, NEW)), "effect {effect} hasn't finished by halfway");
            assert!(differing(&rig.frame(NEW, step(effect, 1.0, false))).all(|p| near(p, NEW)), "effect {effect} ends on the new frame");
        }
    }

    #[test]
    fn what_did_not_change_stays_exact() {
        for effect in 1..=3 {
            let mut rig = Rig::new();
            rig.frame(OLD, None);
            for (i, progress) in [0.0, 0.3, 0.6, 0.9].into_iter().enumerate() {
                let frame = rig.frame(NEW, step(effect, progress, i == 0));
                assert!(frame[SIZE[0] as usize..2 * SIZE[0] as usize].iter().all(|p| *p == SAME), "effect {effect} at {progress}");
            }
        }
    }

    #[test]
    fn only_the_rectangle_is_touched() {
        let mut rig = Rig::new();
        rig.frame(OLD, None);
        let d = Dissolve { effect: 3, rect: [8, 8, 40, 32], progress: 0.0, cell: 8.0, restart: true };
        let frame = rig.frame(NEW, Some(d));
        for (i, p) in frame.iter().enumerate() {
            let (x, y) = (i as u32 % SIZE[0], i as u32 / SIZE[0]);
            if y == 1 {
                continue;
            }
            let inside = (8..40).contains(&x) && (8..32).contains(&y);
            assert!(near(*p, if inside { OLD } else { NEW }), "({x}, {y}) is {p:?}");
        }
    }

    #[test]
    fn restarting_carries_on_from_what_was_on_screen() {
        let mut rig = Rig::new();
        rig.frame(OLD, None);
        let middle = rig.frame(NEW, step(1, 0.5, true));
        // A third frame's transition starts from the half-finished mix, not from the first frame.
        let restarted = rig.frame([0, 200, 0, 255], step(1, 0.0, true));
        for (a, b) in middle.iter().zip(&restarted) {
            assert!(near(*a, *b), "{a:?} became {b:?}");
        }
    }

    #[test]
    fn with_no_frame_kept_it_only_keeps_the_new_one() {
        let mut rig = Rig::new();
        rig.dissolver.forget();
        assert!(differing(&rig.frame(NEW, step(1, 0.5, true))).all(|p| p == NEW));
        // That one is kept for next time.
        assert!(differing(&rig.frame(OLD, step(1, 0.0, true))).all(|p| near(p, NEW)));
    }

    #[test]
    fn the_copies_go_when_the_transition_ends() {
        let mut rig = Rig::new();
        rig.frame(OLD, None);
        rig.frame(NEW, step(1, 0.5, true));
        assert!(rig.dissolver.before.is_some() && rig.dissolver.after.is_some());
        rig.frame(NEW, None);
        assert!(rig.dissolver.before.is_none() && rig.dissolver.after.is_none() && rig.dissolver.shown.is_some());
    }
}
