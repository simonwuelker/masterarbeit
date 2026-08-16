#!/usr/bin/python3

import matplotlib.pyplot as plt
import argparse

parser = argparse.ArgumentParser(prog="PalRup analyzer",
                    description="analyzes the output from the corresponding rust program",
                    epilog="u")

parser.add_argument("--parse-edgelist", nargs="?", const="None", dest="parse_edgelist", help="Analyze the provided edgelist with graphvise")
parser.add_argument("--show-plots", action="store_true", help="Whether matplotlib plots should be shown or just saved")
args = parser.parse_args()


if args.parse_edgelist is not None:
    import networkit as nk
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

def plot_single_histogram(ax, title, histogram_data):
    ax.set_title(title + " size " + str(histogram_data["bucket_size"]))
    x = sorted(map(int, histogram_data["buckets"].keys()))
    y = np.array([histogram_data["buckets"][str(v)] for v in x], dtype=float)

    if np.max(y) / np.min(y) > 50:
        ax.set_yscale("log")
    ax.bar(x, y)

def plot_histograms(data):
    histogram_set = data["histogram_set"]

    fig, axs = plt.subplots(2, 3, constrained_layout = True)

    plot_single_histogram(axs[0, 0], "Incoming Edges", histogram_set["incoming_edges"])
    plot_single_histogram(axs[0, 1], "Outgoing Edges", histogram_set["outgoing_edges"])
    plot_single_histogram(axs[0, 2], "Number of literals", histogram_set["number_of_literals"])
    plot_single_histogram(axs[1, 0], "Clause ID", histogram_set["id"])
    plot_single_histogram(axs[1, 1], "Lifetime", histogram_set["lifetime"])

    plt.savefig("histograms.svg")

def plot_single_2d_histogram(ax, title, histogram_data):
    ax.set_title(title + " size " + str(histogram_data["bucket_size"]))

    buckets = histogram_data["buckets"]
    x_axis_keys = [int(x) for x in buckets.keys()]
    min_y = 2**32
    max_y = 0
    for row in buckets.values():
        min_y = min(min_y, min([int(x) for x in row["buckets"].keys()]))
        max_y = max(max_y, max([int(x) for x in row["buckets"].keys()]))
    print("range", min_y, max_y)
    rows = []
    for i in range(min(x_axis_keys), max(x_axis_keys) + 1):
        row_data = buckets[str(i)]["buckets"]
        row_data = [row_data.get(str(y), 0.0) for y in range(min_y, max_y)]
        rows.append(row_data)

    img = np.array(rows)
    im = ax.imshow(img.transpose())

def plot_2d_histograms(data):
    histogram_set = data["histogram_2d_set"]

    fig, axs = plt.subplots(1, 1, constrained_layout = True)

    plot_single_2d_histogram(axs, "Number of literals over clause id", histogram_set["number_of_literals_over_clause_id"])

    plt.savefig("2d_histograms.svg")

# plot_import_generations(data)
# plot_unused_imports_per_generation(data)
# plot_histograms(data)
plot_2d_histograms(data)
if args.show_plots:
    plt.show()
