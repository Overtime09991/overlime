@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
set PATH=C:\Program Files\LLVM\bin;%PATH%
cd /d C:\dev\chiaki\overlime
clang-cl /nologo /O2 /MD /c overlime_core.c /Foovelime_core.obj -IC:\dev\chiaki\lib\include -IC:\dev\build\lib\include -IC:\vcpkg\installed\x64-windows-static-md\include -D_CRT_SECURE_NO_WARNINGS -Wno-deprecated-declarations || exit /b 1
lib /nologo /OUT:C:\dev\build\overlime_core.lib ovelime_core.obj
