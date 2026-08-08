# vendor/acestep-cpp

Git submodule tới [ace-step/acestep.vst3](https://github.com/ace-step/acestep.vst3) (MIT).
Khởi tạo:

```powershell
git submodule add https://github.com/ace-step/acestep.vst3 vendor/acestep-cpp
git submodule update --init --recursive
```

## Quy tắc

- **CẤM** sửa trực tiếp trong tree submodule. Mọi khác biệt đi qua patch file
  trong `vendor/patches/` + ghi chú tại file này (hiện chưa có patch nào).
- Build (S-01) — CUDA và Vulkan, hai thư mục build riêng:

```powershell
cmake -B build    -DGGML_CUDA=ON   -DCMAKE_BUILD_TYPE=Release
cmake --build build    --config Release -j
cmake -B build-vk -DGGML_VULKAN=ON -DCMAKE_BUILD_TYPE=Release
cmake --build build-vk --config Release -j
```

## Vì sao không fetch-binary

Release binary của upstream không kèm chữ ký mình kiểm chứng được; build từ
source + pin commit trong submodule = biết chính xác mình ship cái gì.
