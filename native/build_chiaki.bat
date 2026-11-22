@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set PATH=C:\Program Files\LLVM\bin;C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\Ninja;C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin;%PATH%
cd /d C:\dev\chiaki
cmake -S . -B C:\dev\build -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER=clang-cl -DCMAKE_CXX_COMPILER=clang-cl -DCMAKE_TOOLCHAIN_FILE=C:/vcpkg/scripts/buildsystems/vcpkg.cmake -DVCPKG_TARGET_TRIPLET=x64-windows-static-md -DVCPKG_MANIFEST_MODE=OFF -DPROTOC=C:/dev/chiaki/third-party/nanopb/generator/protoc.bat -DPYTHON_EXECUTABLE=python -DCHIAKI_ENABLE_GUI=OFF -DCHIAKI_ENABLE_CLI=OFF -DCHIAKI_ENABLE_TESTS=OFF -DCHIAKI_ENABLE_SETSU=OFF -DCHIAKI_ENABLE_STEAMDECK_NATIVE=OFF -DCHIAKI_ENABLE_STEAM_SHORTCUT=OFF -DCHIAKI_USE_SYSTEM_CURL=ON -DCHIAKI_ENABLE_SPEEX=OFF || exit /b 1
cmake --build C:\dev\build --target chiaki-lib -j 8
