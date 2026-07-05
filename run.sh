mkdir instances
# wget "https://benchmark-database.de/getinstances?query=track%3Dmain_2025&context=cnf"   -O track_main_2025.uri
# wget --content-disposition -i track_main_2025.uri -O instances/
wget https://benchmark-database.de/file/0205e2dffaef93a90c239df31755f2e1?context=cnf -O instances/mihal.cnf.xz
# xz --decompress instances/mihal.cnf.xz

# Setup mallob
git clone https://github.com/domschrei/mallob
cd mallob
git checkout fullcmake
mkdir build

./scripts/server/create_mallob_env.sh
./scripts/server/build_and_run_example.sh
git apply patches/0001-Patch-mallob-for-server.patch
