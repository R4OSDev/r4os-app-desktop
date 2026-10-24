//! Current service ABI records, field by field. Native struct padding is
//! never read as initialized Rust bytes. Wire padding is explicitly zero.
use r4os_contract as a;
pub trait Wire: Sized {
    const SIZE: usize;
    fn write(&self, output: &mut [u8]);
    fn read(input: &[u8]) -> Self;
    fn same(&self, other: &Self) -> bool;
}
macro_rules! integer {
    ($($kind:ty),*) => {$ (
        impl Wire for $kind {
            const SIZE: usize = core::mem::size_of::<Self>();
            fn write(&self, output: &mut [u8]) { output[..Self::SIZE].copy_from_slice(&self.to_le_bytes()); }
            fn read(input: &[u8]) -> Self { Self::from_le_bytes(input[..Self::SIZE].try_into().expect("bounded ABI integer")) }
            fn same(&self, other:&Self)->bool {self==other}
        }
    )*};
}
integer!(u8, u16, u32, u64, i16, i32, i64);
impl<T: Wire, const N: usize> Wire for [T; N] {
    const SIZE: usize = T::SIZE * N;
    fn write(&self, output: &mut [u8]) {
        for (i, item) in self.iter().enumerate() {
            item.write(&mut output[i * T::SIZE..][..T::SIZE]);
        }
    }
    fn read(input: &[u8]) -> Self {
        core::array::from_fn(|i| T::read(&input[i * T::SIZE..][..T::SIZE]))
    }
    fn same(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.same(b))
    }
}
fn field<T: Wire>(output: &mut [u8], offset: usize, value: &T) {
    value.write(&mut output[offset..][..T::SIZE]);
}
fn read_field<T: Wire>(input: &[u8], offset: usize, value: &mut T) {
    *value = T::read(&input[offset..][..T::SIZE]);
}
macro_rules! record {
    ($kind:ty; $($field:ident),+ $(,)?) => {
        impl Wire for $kind {
            const SIZE: usize = core::mem::size_of::<Self>();
            fn write(&self, output: &mut [u8]) {
                output[..Self::SIZE].fill(0);
                $(field(output, core::mem::offset_of!(Self,$field), &self.$field);)+
            }
            fn read(input: &[u8]) -> Self {
                let mut value = Self::default();
                $(read_field(input, core::mem::offset_of!(Self,$field), &mut value.$field);)+
                value
            }
            fn same(&self,other:&Self)->bool {true $(&& self.$field.same(&other.$field))+}
        }
    };
}
record!(a::WindowModeIdentity; service, desktop, owner, serial, window_id, reserved);
record!(a::WindowModeExchange; version, size, identity, ack_request_id, ack_result, mode, flags, reserved);
record!(a::WindowModeReply; version, size, identity, request_id, result, phase, mode, requested_mode);
pub fn encode<T: Wire>(value: &T, output: &mut [u8]) -> bool {
    if output.len() != T::SIZE {
        return false;
    }
    value.write(output);
    true
}
pub fn decode<T: Wire>(input: &[u8]) -> Option<T> {
    (input.len() == T::SIZE).then(|| T::read(input))
}

record!(a::ProgramProcessHandle; instance_id, reserved, generation);
record!(a::TrayEvent; magic, version, size, sequence, owner, item_id, item_revision, kind, flags, wheel_delta, x, y, tick, dropped_before, reserved0);
record!(a::TrayServiceRequest; magic, version, size, owner, item_id, item_revision, item_flags, status_flags, after_sequence, deadline_tick, tooltip_length, icon_width, icon_height, icon_format, tooltip, icon, reserved0);
record!(a::TrayDesktopExchange; magic, version, size, desktop_owner, known_revision, cursor, next_cursor, flags, result, registered_count, capacity, desktop_epoch, registry_revision, event, item, reserved0);
record!(a::DisplaySetting; adapter_id, connector_id, flags, receiver, x, y, width, height, scale, rotation, refresh_millihz, clone_group);
record!(a::DisplayLayout; version, size, topology_revision, count, reserved, outputs);
record!(a::DisplayControlRequest; magic, version, size, owner, desktop_epoch, request_id, base_revision, transaction_id, action, reserved, layout);
record!(a::DisplayControlStatus; magic, version, size, desktop_epoch, revision, transaction_id, request_id, owner, deadline_ns, phase, result, flags, reserved, layout);
record!(a::DisplayControlExchange; magic, version, size, desktop_owner, flags, reserved, status, request);
record!(a::GfxOutputId; adapter_id, connector_id, device_generation, connection_generation);
record!(a::GfxHdrMetadata; primaries, white, max_mastering, min_mastering, max_cll, max_fall);
record!(a::GfxColorSignal; version, size, format, bpc, primaries, transfer, range, pipeline, reference_white, peak, black, metadata_valid, metadata, reserved0);
record!(a::AudioOutputInfo; version, size, id, name, kind, availability, flags, preferred_rate, channels, format, reserved);
record!(a::AudioServiceOutputState; magic, version, size, service_epoch, revision, count, total, index, reason, flags, reserved, desired_id, active_id, active_name, outputs);
record!(a::DisplayColorSelection; version, size, output, signal);
record!(a::DisplayColorExchange; base, color);
record!(a::WindowServiceRecord; magic, version, kind, window_id, instance_id, flags, z_order, x, y, w, h, normal_x, normal_y, normal_w, normal_h, title, path, reserved0);
record!(a::WindowServiceResult; magic, version, action, result, flags, window_count, max_windows, focused_window, focused_instance, record, last_error);
record!(a::WindowServiceStatus; magic,version,reserved0,flags,revision,window_count,max_windows,focused_window,focused_instance,next_z,requests,status_requests,snapshot_requests,register_requests,update_requests,focus_requests,minimize_requests,restore_requests,maximize_requests,close_requests,remove_requests,stale_sweeps,restart_cleanups,bad_ops,bad_requests,last_error,reserved1);
record!(a::WindowServiceSnapshot;magic,version,reserved0,status,records);
record!(a::GfxBackendBinding; version,size,adapter_id,milestone,device_generation,reset_generation);
record!(a::GfxBackendProfile; version,size,interface_id_lo,interface_id_hi,revision,data_bytes,data);
record!(a::GfxBackendInfo; version,size,binding,profile,operations,memory_generation);
record!(a::GfxBufferHandle; id,reserved0,generation);
record!(a::GfxBufferReference; version,size,buffer,reference,flags,reserved0);
record!(a::GfxBufferDescriptor; version,size,byte_length,alignment,modifier,width,height,format,plane_count,usage,location,adapter_id,driver_owner,device_generation,plane_offsets,plane_pitches,reserved0);
record!(a::GfxFence; slot,adapter_id,timeline,point,device_generation,reset_generation);
record!(a::WindowGraphicsFormat; format,reserved,color);
record!(a::WindowGraphicsSurface; version,size,service,desktop,owner,window_id,reserved,serial);
record!(a::WindowGraphicsConfig; version,size,revision,width,height,flags,present_modes,min_images,max_images,format_count,reserved,backend,output,display_generation,formats);
record!(a::WindowGraphicsFrame; version,size,surface,chain,config_revision,image_slot,flags,acquire_token,present_serial,source,descriptor,format,ready);
record!(a::WindowGraphicsPublication; version,size,desktop,owner,window_id,action,surface,config);
record!(a::WindowGraphicsConsumer; version,size,desktop,action,reserved,surface,chain,image_slot,result,acquire_token,fence,request_serial);
record!(a::WindowGraphicsReply; version,size,result,flags,revision,surface,chain,image_slot,reserved,acquire_token,wait_fence,config,frame);

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" {
        fn r4desk_wire_check(kind: u32, bytes: *const u8, length: usize) -> u32;
    }
    #[test]
    fn current_tray_and_color_records_match_original_native_abi_without_reading_padding() {
        let mut tray = a::TrayDesktopExchange {
            desktop_owner: a::ProgramProcessHandle {
                instance_id: 86,
                generation: 0x123456789abcdef0,
                reserved: 0,
            },
            known_revision: 0xfedcba9876543210,
            ..a::TrayDesktopExchange::default()
        };
        tray.event.sequence = 42;
        tray.item.icon[255] = 0xabcdef01;
        tray.item.tooltip[63] = b'Z';
        let mut bytes = [0xcc; a::TrayDesktopExchange::SIZE];
        assert!(encode(&tray, &mut bytes));
        assert_eq!(
            unsafe { r4desk_wire_check(1, bytes.as_ptr(), bytes.len()) },
            1
        );
        let copy: a::TrayDesktopExchange = decode(&bytes).unwrap();
        assert_eq!(copy.desktop_owner.generation, tray.desktop_owner.generation);
        assert_eq!(copy.item.icon, tray.item.icon);
        assert_eq!(copy.known_revision, tray.known_revision);
        assert!(decode::<a::TrayDesktopExchange>(&bytes[..bytes.len() - 1]).is_none());
        assert!(!encode(&tray, &mut bytes[..7]));
        let mut color = a::DisplayColorExchange::default();
        color.base.desktop_owner = tray.desktop_owner;
        color.base.status.desktop_epoch = tray.desktop_owner.generation;
        color.base.status.revision = 1;
        color.color.output.connection_generation = 0x1122334455667788;
        color.color.signal.metadata.primaries[5] = 12345;
        let mut bytes = [0xdd; a::DisplayColorExchange::SIZE];
        assert!(encode(&color, &mut bytes));
        assert_eq!(
            unsafe { r4desk_wire_check(2, bytes.as_ptr(), bytes.len()) },
            1
        );
        let copy: a::DisplayColorExchange = decode(&bytes).unwrap();
        assert_eq!(copy.color.signal.metadata.primaries[5], 12345);
        assert_eq!(copy.color.output.connection_generation, 0x1122334455667788);
        let mut consumer = a::WindowGraphicsConsumer {
            desktop: tray.desktop_owner,
            request_serial: 0xfedcba9876543210,
            chain: 55,
            acquire_token: 66,
            image_slot: 2,
            action: a::window_graphics_return,
            ..a::WindowGraphicsConsumer::default()
        };
        consumer.surface.owner = tray.desktop_owner;
        consumer.surface.serial = 42;
        consumer.fence = a::GfxFence {
            slot: 17,
            adapter_id: 1,
            timeline: 55,
            point: 88,
            device_generation: 99,
            reset_generation: 44,
        };
        let mut bytes = [0xdd; a::WindowGraphicsConsumer::SIZE];
        assert!(encode(&consumer, &mut bytes));
        assert_eq!(
            unsafe { r4desk_wire_check(3, bytes.as_ptr(), bytes.len()) },
            1
        );
        let copy: a::WindowGraphicsConsumer = decode(&bytes).unwrap();
        assert!(copy.same(&consumer));
        let mut reply = a::WindowGraphicsReply {
            result: a::window_graphics_ok,
            revision: 0x1122334455667788,
            ..a::WindowGraphicsReply::default()
        };
        reply.frame.surface.owner = tray.desktop_owner;
        reply.frame.acquire_token = 66;
        reply.frame.source.buffer = a::GfxBufferHandle {
            id: 3,
            generation: 11,
            reserved0: 0,
        };
        reply.frame.source.reference = a::GfxBufferHandle {
            id: 4,
            generation: 12,
            reserved0: 0,
        };
        reply.frame.descriptor.plane_pitches[3] = 0x123456789abcdef0;
        reply.frame.format.color[47] = 19;
        reply.config.backend.profile.data[63] = 197;
        reply.config.formats[7].color[47] = 211;
        let mut bytes = [0xdd; a::WindowGraphicsReply::SIZE];
        assert!(encode(&reply, &mut bytes));
        assert_eq!(
            unsafe { r4desk_wire_check(4, bytes.as_ptr(), bytes.len()) },
            1
        );
        let mut copy: a::WindowGraphicsReply = decode(&bytes).unwrap();
        assert!(copy.same(&reply));
        copy.config.formats[7].color[47] ^= 1;
        assert!(!copy.same(&reply));
    }
}
