#!/usr/bin/python3

from os import write
import networkit as nk
import matplotlib.pyplot as plt
import sys
import argparse

parser = argparse.ArgumentParser(prog="PalRup analyzer",
                    description="analyzes the output from the corresponding rust program",
                    epilog="u")

parser.add_argument("--parse-edgelist", nargs="?", const="None", dest="parse_edgelist", help="Analyze the provided edgelist with graphvise")
parser.add_argument("--show-plots", action="store_true", help="Whether matplotlib plots should be shown or just saved")
args = parser.parse_args()


if args.parse_edgelist is not None:
    filename = parser.parse_edgelist
    G = nk.graphio.EdgeListReader("\t", 0, "#", continuous=False).read(filename)
    # G = nk.graphio.EdgeListReader("\t", 0, "#",  directed=True, continuous=False).read("out.edgelist")
    print(nk.overview(G))

    def edgeFunc(u, v, weight, edgeId):
        print("Edge from {} to {} has weight {} and id {}".format(u, v, weight, edgeId))

    print("Computing connected components...")
    cc = nk.components.ConnectedComponents(G)
    cc.run()
    components = [c for c in sorted(cc.getComponents(), key=len) if len(c) != 1 ];

    print("Detecting communities...")
    communities = nk.community.detectCommunities(G)
    print("Number of communities: ", communities.numberOfSubsets())

    sizes = sorted([len(communities.getMembers(index)) for index in range(communities.numberOfSubsets())])
    print(sum([len(communities.getMembers(index)) > 2000 for index in range(communities.numberOfSubsets())]))

    # print("Writing GraphVise graph...")
    # with open("graphvise.txt", "w") as outfile:
    #     v = sum(sizes)
    #     e = sum(1 for _ in G.iterEdges())
    #     outfile.write(f"v={v},e={e}\n")

    #     def callback(u, v, weight, edgeId):
    #         outfile.write(f"{u} {v}\n")
    #     G.forEdges(callback)

    # import random
    # random.seed(42)
    # with open("graphvise_coloring.txt", "w") as outfile:
    #     # v = sum(sizes)
    #     # e = sum(1 for _ in G.iterEdges())
    #     # outfile.write(f"v={v},e={e}\n")
    #     for index in range(communities.numberOfSubsets()):
    #         r = random.randint(0, 255)
    #         g = random.randint(0, 255)
    #         b = random.randint(0, 255)
    #         outfile.write(f"Group Group{index} {r} {g} {b}\n")
    #         for member in communities.getMembers(index):
    #             outfile.write(f"{member}\n")
    #         outfile.write("\n")

    from sqlalchemy import create_engine
    from sqlalchemy.orm import Session
    engine = create_engine("sqlite:///database.db")

    from sqlalchemy import String
    from sqlalchemy.orm import DeclarativeBase
    from sqlalchemy.orm import Mapped
    from sqlalchemy.orm import mapped_column
    from sqlalchemy_utils import database_exists, create_database
    if not database_exists(engine.url):
        create_database(engine.url)

    class Base(DeclarativeBase):
        pass

    class GraphAnalysis(Base):
        __tablename__ = "graph_analysis"
        id: Mapped[int] = mapped_column(primary_key=True)
        filename: Mapped[str] = mapped_column(String(100))
        average_out_degree: Mapped[float]
        global_clustering_coefficient: Mapped[float]

        def __repr__(self) -> str:
            return f"PalRup(id={self.id!r}, name={self.filename!r}"

    GraphAnalysis.__table__.drop(engine)
    Base.metadata.create_all(engine)

    with Session(engine) as session:
        graph_analysis = GraphAnalysis(
            filename=filename,
            average_out_degree=G.numberOfEdges() / G.numberOfNodes(),
            global_clustering_coefficient=nk.globals.clustering(G)
        )
        session.add_all([graph_analysis])
        session.commit()


import json

with open("out.json", "r") as f:
    data = json.load(f)

import numpy as np
import matplotlib.pyplot as plt

def plot_import_generations(data):
    import_depths = np.array([data["per_file"][filename]["import_depths"] for filename in data["per_file"]])
    standard_deviation = np.std(import_depths, axis=0)
    mean = np.mean(import_depths, axis=0)

    fig, ax = plt.subplots()

    ax.bar(range(len(mean)), mean, yerr=standard_deviation, align='center')
    ax.set_ylabel('Percentage of total imports')
    ax.set_title('Maximum generations after import')
    ax.set_xlabel("Generations")
    ax.set_yscale("log")

    plt.savefig("import-generations.svg")

def plot_unused_imports_per_generation(data):
    unused_imports = data["unused_imports"]

    fig, ax = plt.subplots()

    ax.plot(unused_imports, [n / len(unused_imports) for n in range(len(unused_imports))])
    ax.set_ylabel('Percentage of total imports')
    ax.set_title('Unused imports over time')
    ax.set_xlabel("Clause ID")
    # ax.set_yscale("log")
    plt.savefig("unused-imports-per-communication-step.svg")

plot_import_generations(data)
plot_unused_imports_per_generation(data)
if args.show_plots:
    plt.show()
