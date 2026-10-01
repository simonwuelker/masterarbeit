import os
from os import listdir
from os.path import isfile, join
import subprocess
import json
import shutil
import re

def problems(path):
    for filename in listdir(path):
        full_path = join(path, filename)
        if isfile(full_path):
            yield full_path

num_procs = os.cpu_count() // 8
TEMP_DIR = "/nfs/home/swuelker/masterarbeit/temp"
MALLOB = "/nfs/home/swuelker/mallob2"
PROBLEMS = "/nfs/home/swuelker/mallob/problems/"
result = []

for index, problem in enumerate(problems(PROBLEMS)):
    # First, solve the problem
    print("Solving", problem)
    old_cwd = os.getcwd()
    os.chdir(MALLOB)
    command = [
        "mpirun",
        "-np",
        str(num_procs),
        "--bind-to=core",
        "--map-by",
        f"ppr:{num_procs}:node:pe=4",
        join(MALLOB, "build/mallob"),
        "-t=4",
        f"-mono={problem}",
        "-satsolver=c",
        "--palrup",
        f"-proof-dir={TEMP_DIR}",
    ]
    print("Invoking", command)

    env = os.environ.copy()
    env["RDMAV_FORK_SAFE"] = "1"
    env["NPROCS"] = str(num_procs)

    result = subprocess.run(
        command,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )


    os.chdir(old_cwd)

    # Find timing in the logs
    match = re.search(r"RESPONSE_TIME\s+#\d+\s+(\d+(?:\.\d+)?)", result.stdout)
    if not match:
        print("ERROR: No response time found")
        break
    time = float(match.group(1))

    # Then, reduce the proof
    proof_dir = join(TEMP_DIR, [x for x in listdir(TEMP_DIR)][0])
    print("Reducing from", proof_dir)
    destination = join(TEMP_DIR, "stripped")
    cmd = ["cargo", "r", "-r", "--", "strip", f"{proof_dir}", destination]
    print("Invoking", cmd)
    child = subprocess.Popen(
        cmd,
    )

    try:
        child.wait()
    except Exception as exc:
        raise RuntimeError("Waiting for proof stripping") from exc

    # Then, run the depth checker
    cmd = ["cargo", "r", "-r", "--", "depth", destination]
    print("Invoking", cmd)
    child = subprocess.Popen(cmd
    )

    # Read its output file
    print("read json")
    with open("out.json", "r") as result_file:
        data = json.load(result_file)

    data["problem"] = problem
    data["time"] = time

    result.append(data)

    print("Removing results")
    shutil.rmtree(TEMP_DIR)

    if len(result) >= 9:
        break;

print("Writing result file")
with open("measurements.json", "w") as result_file:
    json.dump(result, result_file)
