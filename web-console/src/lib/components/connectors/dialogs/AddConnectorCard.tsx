// Show cards for adding new connectors.
//
// Also attached the dialog for the connector that opens when someone clicks
// on Add.

import { AnyIcon } from '$lib/components/common/AnyIcon'

import { Box } from '@mui/material'
import Button from '@mui/material/Button'
import Card from '@mui/material/Card'
import CardContent from '@mui/material/CardContent'
import Typography from '@mui/material/Typography'

export const AddConnectorCard = (props: {
  id?: string
  icon: string
  title: string
  addInput?: { onClick: () => void } | { href: string }
  addOutput?: { onClick: () => void } | { href: string }
}) => {
  return (
    <Card id={props.id}>
      <CardContent sx={{ textAlign: 'center' }}>
        <AnyIcon
          icon={props.icon}
          fontSize='4rem'
          style={{ marginBottom: 8, height: 64, objectFit: 'cover', width: 'fit-content' }}
        />
        <Typography sx={{ mb: 3 }}>{props.title}</Typography>
        <Box sx={{ display: 'flex', width: '100%' }}>
          {!!props.addInput && (
            <Button variant='contained' size='small' {...props.addInput}>
              Add input
            </Button>
          )}
          {!!props.addOutput && (
            <Button variant='contained' size='small' {...props.addOutput} sx={{ ml: 'auto' }}>
              Add output
            </Button>
          )}
        </Box>
      </CardContent>
    </Card>
  )
}
