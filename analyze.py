#!/usr/bin/python3

import networkit as nk

G = nk.graphio.EdgeListReader("\t", 0, "#").read("out.edgelist")
# G = nk.graphio.EdgeListReader("\t", 0, "#",  directed=True, continuous=False).read("out.edgelist")
print(nk.overview(G))
# G.indexEdges()
# print(G.isDirected())

def edgeFunc(u, v, weight, edgeId):
    print("Edge from {} to {} has weight {} and id {}".format(u, v, weight, edgeId))

# cc = nk.components.WeaklyConnectedComponents(G)
# cc.run()
# print("number of components ", cc.numberOfComponents())
# components = [c for c in sorted(cc.getComponents(), key=len) if len(c) != 1 ];
# print(components)
# G.forEdgesOf(912, edgeFunc)

cc = nk.components.ConnectedComponents(G)
cc.run()
components = [c for c in sorted(cc.getComponents(), key=len) if len(c) != 1 ];
print(len(components))
# print(components)

# print([[x % 48 for x in comp] for comp in components])
