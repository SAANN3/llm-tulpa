//! Names, sizes and sha256 digests of the pinned release's assets, as GitHub lists them for that tag.

pub const PINNED_TAG: &str = "b11347";

/// (asset name, size in bytes, sha256)
pub const PINNED_DIGESTS: &[(&str, u64, &str)] = &[
    ("cudart-llama-b11347-bin-ubuntu-cuda-12.8-x64.tar.gz", 594377795, "44434137b51b0ec626243c8b5cb254780270c7f8e8dae08e0783b601f5432d9c"),
    ("cudart-llama-b11347-bin-ubuntu-cuda-13.4-arm64.tar.gz", 552522171, "2a383ebf254760f4a5bb07c35ca2f533bdbd93d2967cda4e7fdb96cff42fb82a"),
    ("cudart-llama-b11347-bin-ubuntu-cuda-13.4-x64.tar.gz", 440236539, "e3392bd75592c74af651efeb8fb0921249e0e58d77187b181d5b0f3fee307dcf"),
    ("cudart-llama-bin-win-cuda-12.4-x64.zip", 391443627, "8c79a9b226de4b3cacfd1f83d24f962d0773be79f1e7b75c6af4ded7e32ae1d6"),
    ("cudart-llama-bin-win-cuda-13.4-arm64.zip", 153262407, "642dcde8805b3e3165ca710a5443b3b4044b27d96bd3ee3132473988c9bcb774"),
    ("cudart-llama-bin-win-cuda-13.4-x64.zip", 423535356, "738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668"),
    ("llama-b11347-bin-linux-arm64-snapdragon.tar.gz", 19269214, "3140908ca72983d9793e7a20fd5d6f874564dbeb06aedbe2386c6ffbb8c74f70"),
    ("llama-b11347-bin-macos-arm64.tar.gz", 11828974, "0009fd608997a47f931afef26ceab1d3ba7e9afebf816c917b48ef43b123b59b"),
    ("llama-b11347-bin-macos-x64.tar.gz", 11383816, "e3603f89685ad94a1870ab0cf414394c8a31dc8b7a53323f5eb4ce1dfbcd0d51"),
    ("llama-b11347-bin-ubuntu-arm64.tar.gz", 13591763, "c7e30d36b2d26d1469e823e5edffce42498616d500ab8a499748913937095aef"),
    ("llama-b11347-bin-ubuntu-cuda-12.8-x64.tar.gz", 171244141, "1ba8bdb1725c88005c9780ad61918fd9cdaa14fe7bc18da892267c0888c047e2"),
    ("llama-b11347-bin-ubuntu-cuda-13.4-arm64.tar.gz", 147414544, "611ae976727c8c3da39d1e2425999776efc5c67a2259d4058718157dc040aa6e"),
    ("llama-b11347-bin-ubuntu-cuda-13.4-x64.tar.gz", 152148869, "6dbb48ae030b5ba6205eb6061057f549fef5f7547c637b9e81845f80b677ad53"),
    ("llama-b11347-bin-ubuntu-openvino-2026.4-x64.tar.gz", 109148734, "48a88f0583264395f1f3159bc6c529a583cc52d55e7d9e2fc02d7d5f2e16e222"),
    ("llama-b11347-bin-ubuntu-rocm-10.0-x64.tar.gz", 243027170, "daa8c90cc47e0621711d338c49ee4caeff14735d21ccb956be81122ddd81433b"),
    ("llama-b11347-bin-ubuntu-s390x.tar.gz", 15697373, "fbbb83601d246c7697a24db41889e36c465498fdf418b4fee3d3fc11d715741f"),
    ("llama-b11347-bin-ubuntu-sycl-fp16-x64.tar.gz", 54173085, "11cf7f01d657a264a7e7e35068fc8142f3f042f090eb3319fd86d8229466b7ef"),
    ("llama-b11347-bin-ubuntu-sycl-fp32-x64.tar.gz", 54067195, "4c1744906bb47318b70fec40163ddece07e45bd72138a1f6c11d46305e62f225"),
    ("llama-b11347-bin-ubuntu-vulkan-arm64.tar.gz", 24751177, "8a6bb21f5ba412c833dae6c4697a1c11c24b9bba06a13253c8c31a28e3e2fc4d"),
    ("llama-b11347-bin-ubuntu-vulkan-x64.tar.gz", 31497153, "85b28322de2ac4b268fba4c1737e2824e049dcc9bda19c782538d7c7b6033a09"),
    ("llama-b11347-bin-ubuntu-x64.tar.gz", 17551840, "a21ad95ccd452fc22b29c6badf126a9186612dccc00260373dea68d3932eb97d"),
    ("llama-b11347-bin-win-cpu-arm64.zip", 12119185, "ac52b0a14fe28ef7435794a43eb8406c6880d5aded246745a092713830b2e4a8"),
    ("llama-b11347-bin-win-cpu-x64.zip", 19275609, "f1f7198a7c1cdf13bec7c2cf960947e0bec87401837a057134186132d870aa04"),
    ("llama-b11347-bin-win-cuda-12.4-x64.zip", 263289044, "905699e995c91b49408b74b8375fd16a2b308e6015d52b2b506e6aea75addbeb"),
    ("llama-b11347-bin-win-cuda-13.4-arm64.zip", 144946916, "c2b40708e175ccdc418e7d3866606b83733f04138a869aa7ba2f15691cc3b96f"),
    ("llama-b11347-bin-win-cuda-13.4-x64.zip", 152793721, "876e69e48b3b7208bf75e92c4b93a908fca51fb7de3b329dc76bba67f60b7011"),
    ("llama-b11347-bin-win-opencl-adreno-arm64.zip", 12934108, "ec4205f4c3ec94f908c019ee0334ec20a694f88abd882e75eca4e714d45b30bf"),
    ("llama-b11347-bin-win-openvino-2026.4-x64.zip", 88397764, "19ce3f4f7f0d9a21e4c32926bc4a4f0781a4f697fa9d3e4f4bbbcd9f9b573e74"),
    ("llama-b11347-bin-win-rocm-10.0-x64.zip", 256208958, "d948f936ff2bf314132f0ea4c2694ead3896be32525f88b8687b9dda8090930a"),
    ("llama-b11347-bin-win-sycl-x64.zip", 147927187, "d321bff530e3edc7abc55cd02e0f819e63ad8a9b0c1be1ca4b67ef259ea04169"),
    ("llama-b11347-bin-win-vulkan-x64.zip", 33209139, "7ecd8b4fb5a404477e6a12f04ace0ab834fc492c197d06fb649589a7fcdccd29"),
];
