contract CStoreAndLoad {
    uint256 constant SLOT = 0x444;
    uint256 constant VALUE = 0x123456789;

    function test() public {
        uint256 x;
        assembly {
            cstore(SLOT, VALUE)
            x := cload(SLOT)
        }
        assert(x == VALUE);
    }
}