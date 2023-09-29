// InputNodes are on the left and connect to tables of the program.

import { AnyIcon } from '$lib/components/common/AnyIcon'
import { Handle, Node } from '$lib/components/streaming/builder/NodeTypes'
import useNodeDelete from '$lib/compositions/streaming/builder/useNodeDelete'
import { connectorDescrToType, connectorTypeToIcon } from '$lib/functions/connectors'
import { ConnectorDescr } from '$lib/services/manager'
import { Connection, getConnectedEdges, NodeProps, Position, useReactFlow } from 'reactflow'

import { Icon } from '@iconify/react'
import { Box, Link } from '@mui/material'
import Avatar from '@mui/material/Avatar'
import CardHeader from '@mui/material/CardHeader'
import IconButton from '@mui/material/IconButton'

const InputNode = ({ id, data }: NodeProps<{ connector: ConnectorDescr }>) => {
  const { getNode, getEdges, deleteElements } = useReactFlow()
  const onDelete = useNodeDelete(id)

  const isValidConnection = (connection: Connection) => {
    // We drop the other edge if there already is one (no more than one outgoing
    // connection from each input).
    if (connection.source) {
      const sourceNode = getNode(connection.source)
      if (sourceNode !== undefined) {
        const edges = getConnectedEdges([sourceNode], getEdges())
        deleteElements({ nodes: [], edges })
      }
    }

    // Only allow the connection if we're going to a table.
    if (connection.target) {
      const targetNode = getNode(connection.target)

      return (
        targetNode !== undefined &&
        targetNode.type === 'sqlProgram' &&
        connection.targetHandle != null &&
        connection.targetHandle.startsWith('table-')
      )
    } else {
      return false
    }
  }

  return (
    <Node>
      <Link href={`#edit/connector/${data.connector.connector_id}`}>
        <CardHeader
          title={data.connector.name}
          subheader={data.connector.description}
          sx={{ py: 5, pr: 12, alignItems: 'flex-start' }}
          titleTypographyProps={{ variant: 'h5' }}
          subheaderTypographyProps={{ variant: 'body1', sx: { color: 'text.disabled' } }}
          avatar={
            <Avatar variant='rounded' sx={{ mt: 1.5, width: 42, height: 42 }}>
              <AnyIcon
                icon={connectorTypeToIcon(connectorDescrToType(data.connector))}
                style={{ width: '90%', height: '90%' }}
              />
            </Avatar>
          }
        />
      </Link>
      <Box
        sx={{ display: 'flex', alignItems: 'center', position: 'absolute', top: 4, right: 4 }}
        className='nodrag nopan'
      >
        <IconButton size='small' aria-label='close' sx={{ color: 'text.secondary' }} onClick={() => onDelete()}>
          <Icon icon='bx:x' fontSize={20} />
        </IconButton>
      </Box>
      {/* The .inputHandle is referenced by webui-tester */}
      <Handle
        className='inputHandle'
        type='source'
        position={Position.Right}
        isConnectable={true}
        isValidConnection={isValidConnection}
      />
    </Node>
  )
}

export default InputNode
