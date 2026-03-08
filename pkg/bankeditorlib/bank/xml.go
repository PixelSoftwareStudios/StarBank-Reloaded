package bank

import "encoding/xml"

// xmlBank is the top-level structure of a .SC2Bank file.
type xmlBank struct {
	XMLName   xml.Name      `xml:"Bank"`
	Version   string        `xml:"version,attr"`
	Sections  []xmlSection  `xml:"Section"`
	Signature *xmlSignature `xml:"Signature"`
}

// xmlSignature represents <Signature value="..."/>.
type xmlSignature struct {
	Value string `xml:"value,attr"`
}

// xmlSection represents <Section name="...">.
type xmlSection struct {
	Name string   `xml:"name,attr"`
	Keys []xmlKey `xml:"Key"`
}

// xmlKey represents:
//
//	<Key name="...">
//	    <Value type="value"/>
//	</Key>
type xmlKey struct {
	Name  string   `xml:"name,attr"`
	Value xmlValue `xml:"Value"`
}

// xmlValue holds a <Value type="val"/> element where the attribute NAME is the
// value type and the attribute VALUE is the actual data. Standard encoding/xml
// cannot handle a dynamic attribute name, so we implement custom marshal/unmarshal.
type xmlValue struct {
	Type  string
	Value string
}

func (v *xmlValue) UnmarshalXML(d *xml.Decoder, start xml.StartElement) error {
	if len(start.Attr) > 0 {
		v.Type = start.Attr[0].Name.Local
		v.Value = start.Attr[0].Value
	}
	return d.Skip()
}

func (v xmlValue) MarshalXML(e *xml.Encoder, start xml.StartElement) error {
	start.Name.Local = "Value"
	if v.Type != "" {
		start.Attr = []xml.Attr{
			{Name: xml.Name{Local: v.Type}, Value: v.Value},
		}
	} else {
		start.Attr = nil
	}
	if err := e.EncodeToken(start); err != nil {
		return err
	}
	return e.EncodeToken(start.End())
}
